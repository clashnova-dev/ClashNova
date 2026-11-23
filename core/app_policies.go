package main

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httputil"
	"net/url"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unicode/utf8"

	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing-box/common/trafficcontrol"
	"github.com/sagernet/sing-box/experimental"
	"github.com/sagernet/sing-box/experimental/clashapi"
	"github.com/sagernet/sing-box/log"
	"github.com/sagernet/sing-box/option"
	"github.com/sagernet/sing/service"
)

const policyPrefix = "clashnova-app-"

var policyKinds = []string{"block", "proxy", "direct"}

type policyContextKey struct{}
type policyMatchers map[string][]*regexp.Regexp
type policyState struct {
	current atomic.Pointer[policyMatchers]
	update  sync.Mutex
}

func (p policyMatchers) policy(path string) string {
	for _, kind := range policyKinds {
		for _, pattern := range p[kind] {
			if pattern.MatchString(path) {
				return kind
			}
		}
	}
	return "inherit"
}

func appExpression(app string) string {
	name := regexp.QuoteMeta(app)
	if strings.HasSuffix(strings.ToLower(app), ".exe") {
		return `(?i)(?:^|[\\/])` + name + `$`
	}
	return `(?:^|/)` + name + `(?:\.app/|$)`
}

func compilePolicies(policies map[string]string) (policyMatchers, error) {
	if len(policies) > 1024 {
		return nil, errors.New("too many application policies")
	}
	result := make(policyMatchers)
	for app, kind := range policies {
		if app == "" || utf8.RuneCountInString(app) > 256 || strings.ContainsAny(app, "/\\\x00\r\n") {
			return nil, errors.New("invalid application name")
		}
		if kind != "block" && kind != "proxy" && kind != "direct" {
			return nil, errors.New("invalid application policy")
		}
		result[kind] = append(result[kind], regexp.MustCompile(appExpression(app)))
	}
	return result, nil
}

// These three slots are reserved by the client, never changed after startup.
// Requests only swap an immutable matcher snapshot; router slices remain untouched.
func prepareAppPolicies(ctx context.Context, options *option.Options) (context.Context, error) {
	if options.Route == nil {
		return ctx, nil
	}
	slots := make(map[string]option.Rule)
	var other []option.Rule
	for _, rule := range options.Route.Rules {
		if len(rule.DefaultOptions.Inbound) != 1 || !strings.HasPrefix(rule.DefaultOptions.Inbound[0], policyPrefix) {
			other = append(other, rule)
			continue
		}
		kind := strings.TrimPrefix(rule.DefaultOptions.Inbound[0], policyPrefix)
		if kind != "block" && kind != "proxy" && kind != "direct" {
			other = append(other, rule)
			continue
		}
		if _, exists := slots[kind]; exists {
			return nil, errors.New("duplicate application policy slot")
		}
		slots[kind] = rule
	}
	if len(slots) == 0 {
		return ctx, nil
	}
	if len(slots) != 3 {
		return nil, errors.New("incomplete application policy slots")
	}
	initial := make(policyMatchers)
	var ordered []option.Rule
	for _, kind := range policyKinds {
		rule := slots[kind].DefaultOptions
		ordered = append(ordered, slots[kind])
		for _, expression := range rule.ProcessPathRegex {
			pattern, err := regexp.Compile(expression)
			if err != nil {
				return nil, fmt.Errorf("application policy: %w", err)
			}
			initial[kind] = append(initial[kind], pattern)
		}
	}
	// App blocks also precede TUN's STUN/private-network routing exceptions.
	options.Route.Rules = append(ordered, other...)
	state := new(policyState)
	state.current.Store(&initial)
	return context.WithValue(ctx, policyContextKey{}, state), nil
}

type livePolicyRule struct {
	adapter.Rule
	state *policyState
	kind  string
}

func (r *livePolicyRule) Match(metadata *adapter.InboundContext) bool {
	return metadata.ProcessInfo != nil && metadata.ProcessInfo.ProcessPath != "" && r.state.current.Load().policy(metadata.ProcessInfo.ProcessPath) == r.kind
}
func (r *livePolicyRule) String() string { return "application-policy=" + r.kind }

func init() { experimental.RegisterClashServerConstructor(newPolicyController) }

type policyController struct {
	adapter.LifecycleService
	server    *http.Server
	transport *http.Transport
}

func (s *policyController) Start(stage adapter.StartStage, scope *adapter.Scope) error {
	if err := s.LifecycleService.Start(stage, scope); err != nil {
		return err
	}
	if stage != adapter.StartStateStarted {
		return nil
	}
	listener, err := net.Listen("tcp", s.server.Addr)
	if err != nil {
		return err
	}
	scope.Add(s.server.Close)
	scope.Add(func() error { s.transport.CloseIdleConnections(); return nil })
	go func() { _ = s.server.Serve(listener) }()
	return nil
}

func newPolicyController(ctx context.Context, logs log.ObservableFactory, options option.ClashAPIOptions) (adapter.LifecycleService, error) {
	state, ok := ctx.Value(policyContextKey{}).(*policyState)
	if !ok {
		return clashapi.NewServer(ctx, logs, options)
	}
	address, err := net.ResolveTCPAddr("tcp", options.ExternalController)
	if err != nil || !address.IP.IsLoopback() || address.Port < 1024 || len(options.Secret) < 32 {
		return nil, errors.New("application controller must use an authenticated loopback address")
	}
	router := service.FromContext[adapter.Router](ctx)
	rules := router.Rules()
	for i, kind := range policyKinds {
		rules[i] = &livePolicyRule{Rule: rules[i], state: state, kind: kind}
	}
	router.AppendTracker(&policyGuard{state: state})
	// Keep sing-box's API intact, with a private internal listener. Proxying affects
	// only controller observations; application traffic never traverses this server.
	var innerAddress string
	for attempt := 0; attempt < 3; attempt++ {
		socket, err := net.Listen("tcp", "127.0.0.1:0")
		if err != nil {
			return nil, err
		}
		candidate := socket.Addr().(*net.TCPAddr)
		_ = socket.Close()
		if candidate.Port != address.Port {
			innerAddress = candidate.String()
			break
		}
	}
	if innerAddress == "" {
		return nil, errors.New("no internal controller port available")
	}
	innerOptions := options
	innerOptions.ExternalController = innerAddress
	inner, err := clashapi.NewServer(ctx, logs, innerOptions)
	if err != nil {
		return nil, err
	}
	transport := &http.Transport{Proxy: nil, DialContext: (&net.Dialer{Timeout: 2 * time.Second}).DialContext}
	proxy := httputil.NewSingleHostReverseProxy(&url.URL{Scheme: "http", Host: innerAddress})
	proxy.Transport = transport
	proxy.FlushInterval = -1
	mux := http.NewServeMux()
	mux.Handle("/", proxy)
	mux.Handle("/app-policies", policyHandler(state, options.Secret, service.PtrFromContext[trafficcontrol.Manager](ctx)))
	return &policyController{LifecycleService: inner, transport: transport, server: &http.Server{
		Addr: options.ExternalController, Handler: mux, ReadHeaderTimeout: 5 * time.Second, IdleTimeout: 30 * time.Second, MaxHeaderBytes: 16 * 1024,
	}}, nil
}

func policyHandler(state *policyState, secret string, traffic *trafficcontrol.Manager) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if subtle.ConstantTimeCompare([]byte(r.Header.Get("Authorization")), []byte("Bearer "+secret)) != 1 {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		if r.Method != http.MethodPut {
			w.Header().Set("Allow", "PUT")
			http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
			return
		}
		var policies map[string]string
		decoder := json.NewDecoder(http.MaxBytesReader(w, r.Body, 256*1024))
		if err := decoder.Decode(&policies); err != nil || policies == nil {
			http.Error(w, "invalid policies", http.StatusBadRequest)
			return
		}
		if decoder.Decode(new(any)) != io.EOF {
			http.Error(w, "invalid policies", http.StatusBadRequest)
			return
		}
		next, err := compilePolicies(policies)
		if err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		state.update.Lock()
		defer state.update.Unlock()
		previous := state.current.Swap(&next)
		closed := 0
		if traffic != nil {
			for _, connection := range traffic.Connections() {
				process := connection.Metadata.ProcessInfo
				if process == nil || process.ProcessPath == "" {
					continue
				}
				before, after := previous.policy(process.ProcessPath), next.policy(process.ProcessPath)
				if before == after && after != "block" {
					continue
				}
				if tracker := traffic.Connection(connection.ID); tracker != nil {
					if err := tracker.Close(); err != nil && !errors.Is(err, net.ErrClosed) {
						state.current.Store(previous)
						http.Error(w, "could not interrupt application connection", http.StatusInternalServerError)
						return
					}
					closed++
				}
			}
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{"applied": true, "closed": closed})
	})
}
