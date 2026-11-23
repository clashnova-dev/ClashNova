package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing-box/common/trafficcontrol"
	"github.com/sagernet/sing-box/option"
	SJSON "github.com/sagernet/sing/common/json"
	N "github.com/sagernet/sing/common/network"
)

func policyTestState(t *testing.T) *policyState {
	t.Helper()
	value, err := compilePolicies(map[string]string{})
	if err != nil {
		t.Fatal(err)
	}
	state := new(policyState)
	state.current.Store(&value)
	return state
}
func policyRequest(handler http.Handler, body string, authorized bool) *httptest.ResponseRecorder {
	r := httptest.NewRequest(http.MethodPut, "http://localhost/app-policies", strings.NewReader(body))
	if authorized {
		r.Header.Set("Authorization", "Bearer test-secret")
	}
	w := httptest.NewRecorder()
	handler.ServeHTTP(w, r)
	return w
}
func owner(path string) adapter.InboundContext {
	return adapter.InboundContext{ProcessInfo: &adapter.ConnectionOwner{ProcessPath: path}}
}

type testOutbound struct{ adapter.Outbound }

func (testOutbound) Tag() string  { return "direct" }
func (testOutbound) Type() string { return "direct" }

func TestPolicyValidationAndLiteralBoundaries(t *testing.T) {
	value, err := compilePolicies(map[string]string{"Acme [Pro]": "block", "CHROME.EXE": "direct"})
	if err != nil {
		t.Fatal(err)
	}
	for _, path := range []string{"/Applications/Acme [Pro].app/Contents/MacOS/Helper", "/usr/bin/Acme [Pro]"} {
		if value.policy(path) != "block" {
			t.Fatal(path)
		}
	}
	if value.policy(`C:\Apps\chrome.exe`) != "direct" || value.policy("/Applications/NotAcme [Pro].app/Contents/MacOS/Helper") != "inherit" {
		t.Fatal("incorrect process boundary")
	}
	for _, bad := range []map[string]string{{"": "block"}, {"../app": "block"}, {"app": "script"}, {"app\n": "block"}} {
		if _, err := compilePolicies(bad); err == nil {
			t.Fatal("invalid policy accepted")
		}
	}
	state := policyTestState(t)
	handler := policyHandler(state, "test-secret", nil)
	for _, body := range []string{`null`, `{"app":"script"}`, `{"app":"block"} {}`} {
		if policyRequest(handler, body, true).Code != http.StatusBadRequest {
			t.Fatalf("accepted %s", body)
		}
		if state.current.Load().policy("/usr/bin/app") != "inherit" {
			t.Fatal("invalid request partially applied")
		}
	}
	if policyRequest(handler, `{"app":"block"}`, false).Code != http.StatusUnauthorized {
		t.Fatal("unauthenticated policy change")
	}
}

func TestPolicyUpdateClosesOnlyAffectedStreamsAndCanBeUndone(t *testing.T) {
	state := policyTestState(t)
	traffic := trafficcontrol.NewManager()
	defer traffic.CloseAllConnections()
	ctx := context.Background()
	out := testOutbound{}
	tracked := func(path string) (net.Conn, net.Conn) {
		a, b := net.Pipe()
		t.Cleanup(func() { a.Close(); b.Close() })
		metadata := owner(path)
		metadata.OutboundChain = []adapter.Outbound{out}
		return traffic.RoutedConnection(ctx, a, metadata, nil, out), b
	}
	_, blocked := tracked("/Applications/Cursor.app/Contents/MacOS/Cursor")
	other, allowed := tracked("/Applications/Other.app/Contents/MacOS/Other")
	handler := policyHandler(state, "test-secret", traffic)
	response := policyRequest(handler, `{"Cursor":"block"}`, true)
	if response.Code != http.StatusOK {
		t.Fatal(response.Body.String())
	}
	blocked.SetReadDeadline(time.Now().Add(time.Second))
	if _, err := blocked.Read(make([]byte, 1)); err != io.EOF {
		t.Fatalf("stream remained open: %v", err)
	}
	allowed.SetReadDeadline(time.Now().Add(time.Second))
	go other.Write([]byte("x"))
	if _, err := allowed.Read(make([]byte, 1)); err != nil {
		t.Fatal("unrelated application was interrupted", err)
	}
	rule := &livePolicyRule{state: state, kind: "block"}
	metadata := owner("/Applications/Cursor.app/Contents/Frameworks/Helper.app/Contents/MacOS/Helper")
	if !rule.Match(&metadata) {
		t.Fatal("new connection not blocked immediately")
	}
	// A stream routed before the update but registered after its close snapshot
	// is still blocked by the final guard, including TUN flow attachment.
	late, peer := net.Pipe()
	defer peer.Close()
	(&policyGuard{state: state}).RoutedConnection(ctx, late, metadata, nil, out)
	peer.SetReadDeadline(time.Now().Add(time.Second))
	if _, err := peer.Read(make([]byte, 1)); err != io.EOF {
		t.Fatal("late connection escaped")
	}
	flow := &testPolicyFlow{}
	(&policyGuard{state: state}).RoutedFlow(ctx, metadata, nil, out).AttachFlow(flow)
	if !flow.closed {
		t.Fatal("late TUN flow escaped")
	}
	packet := &testPolicyPacket{}
	(&policyGuard{state: state}).RoutedPacketConnection(ctx, packet, metadata, nil, out)
	if !packet.closed {
		t.Fatal("late UDP connection escaped")
	}
	if policyRequest(handler, `{}`, true).Code != http.StatusOK || rule.Match(&metadata) {
		t.Fatal("unblocking did not take effect")
	}
}

type testPolicyFlow struct{ closed bool }

func (f *testPolicyFlow) CloseFlow() { f.closed = true }

type testPolicyPacket struct {
	N.PacketConn
	closed bool
}

func (p *testPolicyPacket) Close() error { p.closed = true; return nil }

type failingPolicyConnection struct{ net.Conn }

func (*failingPolicyConnection) Close() error { return errors.New("fixture close failure") }

func TestPolicyCloseFailureDoesNotReportAnAppliedBlock(t *testing.T) {
	state := policyTestState(t)
	traffic := trafficcontrol.NewManager()
	a, b := net.Pipe()
	defer a.Close()
	defer b.Close()
	metadata := owner("/usr/bin/app")
	out := testOutbound{}
	metadata.OutboundChain = []adapter.Outbound{out}
	traffic.RoutedConnection(context.Background(), &failingPolicyConnection{a}, metadata, nil, out)
	response := policyRequest(policyHandler(state, "test-secret", traffic), `{"app":"block"}`, true)
	if response.Code != http.StatusInternalServerError {
		t.Fatal("close failure reported success")
	}
	if state.current.Load().policy("/usr/bin/app") != "inherit" {
		t.Fatal("failed policy update not rolled back")
	}
}

func TestPolicySnapshotsAreSafeDuringConcurrentRouting(t *testing.T) {
	state := policyTestState(t)
	handler := policyHandler(state, "test-secret", nil)
	var group sync.WaitGroup
	for i := 0; i < 4; i++ {
		group.Add(1)
		go func() {
			defer group.Done()
			metadata := owner("/usr/bin/app")
			rule := &livePolicyRule{state: state, kind: "block"}
			for j := 0; j < 1000; j++ {
				rule.Match(&metadata)
			}
		}()
	}
	for i := 0; i < 40; i++ {
		body := `{}`
		if i%2 == 0 {
			body = `{"app":"block"}`
		}
		if policyRequest(handler, body, true).Code != http.StatusOK {
			t.Fatal("update rejected")
		}
	}
	group.Wait()
}

func TestPolicySlotsPrecedeTunExceptionsAndSurviveRestart(t *testing.T) {
	content := `{"route":{"rules":[{"protocol":"stun","outbound":"select"},{"inbound":["clashnova-app-block"],"process_path_regex":["(?:^|/)Cursor(?:\\.app/|$)"],"action":"reject"},{"inbound":["clashnova-app-proxy"],"process_path_regex":["^$"],"outbound":"select"},{"inbound":["clashnova-app-direct"],"process_path_regex":["^$"],"outbound":"direct"}]}}`
	options, err := SJSON.UnmarshalExtended[option.Options]([]byte(content))
	if err != nil {
		t.Fatal(err)
	}
	ctx, err := prepareAppPolicies(context.Background(), &options)
	if err != nil {
		t.Fatal(err)
	}
	state := ctx.Value(policyContextKey{}).(*policyState)
	if options.Route.Rules[0].DefaultOptions.Inbound[0] != "clashnova-app-block" || state.current.Load().policy("/Applications/Cursor.app/Contents/MacOS/Cursor") != "block" {
		t.Fatal("startup block lost or behind STUN exception")
	}
}

func freePolicyPort(t *testing.T) int {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	p := l.Addr().(*net.TCPAddr).Port
	l.Close()
	return p
}

func TestLivePolicyControllerKeepsCoreRunningAndCutsProxyStream(t *testing.T) {
	apiPort, proxyPort := freePolicyPort(t), freePolicyPort(t)
	secret := strings.Repeat("a", 32)
	content := fmt.Sprintf(`{"log":{"level":"error"},"inbounds":[{"type":"mixed","listen":"127.0.0.1","listen_port":%d}],"outbounds":[{"type":"direct","tag":"direct"},{"type":"selector","tag":"select","outbounds":["direct"]}],"route":{"find_process":true,"final":"select","rules":[{"inbound":["clashnova-app-block"],"process_path_regex":["^$"],"action":"reject"},{"inbound":["clashnova-app-proxy"],"process_path_regex":["^$"],"outbound":"select"},{"inbound":["clashnova-app-direct"],"process_path_regex":["^$"],"outbound":"direct"}]},"experimental":{"clash_api":{"external_controller":"127.0.0.1:%d","secret":"%s"}}}`, proxyPort, apiPort, secret)
	instance, cancel, err := create(writeConfig(t, content))
	if err != nil {
		t.Fatal(err)
	}
	defer cancel()
	defer instance.Close()
	if err = instance.Start(); err != nil {
		t.Fatal(err)
	}
	var reached atomic.Int32
	target := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		reached.Add(1)
		if r.URL.Path == "/stream" {
			io.WriteString(w, "hello\n")
			w.(http.Flusher).Flush()
			<-r.Context().Done()
			return
		}
		io.WriteString(w, "ok")
	}))
	defer target.Close()
	proxyURL, _ := url.Parse(fmt.Sprintf("http://127.0.0.1:%d", proxyPort))
	transport := &http.Transport{Proxy: http.ProxyURL(proxyURL), DisableKeepAlives: true}
	defer transport.CloseIdleConnections()
	through := &http.Client{Transport: transport, Timeout: 3 * time.Second}
	direct := &http.Client{Transport: &http.Transport{Proxy: nil}, Timeout: 3 * time.Second}
	api := func(method, path, body string) *http.Response {
		t.Helper()
		req, _ := http.NewRequest(method, fmt.Sprintf("http://127.0.0.1:%d%s", apiPort, path), strings.NewReader(body))
		req.Header.Set("Authorization", "Bearer "+secret)
		r, err := direct.Do(req)
		if err != nil {
			t.Fatal(err)
		}
		return r
	}
	stream, err := through.Get(target.URL + "/stream")
	if err != nil {
		t.Fatal(err)
	}
	defer stream.Body.Close()
	if _, err = io.ReadFull(stream.Body, make([]byte, 6)); err != nil {
		t.Fatal(err)
	}
	connections := api("GET", "/connections", "")
	var frame struct {
		Connections []struct{ Metadata struct{ ProcessPath string } }
	}
	if err = json.NewDecoder(connections.Body).Decode(&frame); err != nil {
		t.Fatal(err)
	}
	connections.Body.Close()
	path := ""
	for _, row := range frame.Connections {
		if row.Metadata.ProcessPath != "" {
			path = row.Metadata.ProcessPath
			break
		}
	}
	if path == "" {
		t.Skip("OS process ownership unavailable; metadata-based routing and stream-close tests still required")
	}
	executable, _ := os.Executable()
	app := filepath.Base(executable)
	if !regexp.MustCompile(appExpression(app)).MatchString(strings.Split(path, " (")[0]) {
		t.Fatalf("unexpected process identity %q", path)
	}
	body, _ := json.Marshal(map[string]string{app: "block"})
	changed := api("PUT", "/app-policies", string(body))
	if changed.StatusCode != 200 {
		t.Fatal(changed.Status)
	}
	changed.Body.Close()
	if _, err = stream.Body.Read(make([]byte, 1)); err == nil {
		t.Fatal("blocked proxy stream remained usable")
	}
	if r, err := through.Get(target.URL); err == nil {
		status := r.StatusCode
		r.Body.Close()
		if status < 400 {
			t.Fatalf("new proxy request escaped block: HTTP %d", status)
		}
	}
	if reached.Load() != 1 {
		t.Fatal("blocked request reached the destination")
	}
	cleared := api("PUT", "/app-policies", `{}`)
	if cleared.StatusCode != 200 {
		t.Fatal(cleared.Status)
	}
	cleared.Body.Close()
	r, err := through.Get(target.URL)
	if err != nil {
		t.Fatal("unblock failed without restart", err)
	}
	r.Body.Close()
	version := api("GET", "/version", "")
	defer version.Body.Close()
	if version.StatusCode != 200 {
		t.Fatal("original controller no longer works")
	}
}
