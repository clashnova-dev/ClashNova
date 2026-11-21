// Command airport-core is the desktop client's sing-box core.
//
// It links sing-box as a library and is built from this source for every desktop
// target, so no prebuilt per-platform core binary has to be fetched or shipped.
// The command line is the small part of sing-box's own that the client uses:
//
//	airport-core run -c config.json
//	airport-core check -c config.json
//	airport-core version
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"io"
	"os"
	"os/signal"
	"runtime"
	"runtime/debug"
	"syscall"
	"time"

	box "github.com/sagernet/sing-box"
	C "github.com/sagernet/sing-box/constant"
	"github.com/sagernet/sing-box/experimental/deprecated"
	"github.com/sagernet/sing-box/include"
	"github.com/sagernet/sing-box/log"
	"github.com/sagernet/sing-box/option"
	"github.com/sagernet/sing/common/json"
	"github.com/sagernet/sing/service"
)

const usage = "usage: airport-core run -c <config> | check -c <config> | version"

func main() {
	if err := execute(os.Args[1:], os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func execute(args []string, stdout io.Writer) error {
	if len(args) == 0 {
		return errors.New(usage)
	}
	switch args[0] {
	case "run", "check":
		path, err := configPath(args[0], args[1:])
		if err != nil {
			return err
		}
		if args[0] == "check" {
			return check(path)
		}
		return run(path)
	case "version":
		if len(args) != 1 {
			return errors.New(usage)
		}
		fmt.Fprintf(stdout, "airport-core (sing-box %s)\n%s %s/%s\ntags: %s\n", C.Version, runtime.Version(), runtime.GOOS, runtime.GOARCH, buildTags())
		return nil
	default:
		return errors.New(usage)
	}
}

func configPath(command string, args []string) (string, error) {
	flags := flag.NewFlagSet(command, flag.ContinueOnError)
	flags.SetOutput(io.Discard)
	var path string
	flags.StringVar(&path, "c", "", "configuration file path")
	flags.StringVar(&path, "config", "", "configuration file path")
	if err := flags.Parse(args); err != nil || path == "" || flags.NArg() != 0 {
		return "", errors.New(usage)
	}
	return path, nil
}

func buildTags() string {
	if info, ok := debug.ReadBuildInfo(); ok {
		for _, setting := range info.Settings {
			if setting.Key == "-tags" {
				return setting.Value
			}
		}
	}
	return ""
}

// create parses the configuration and builds the service without starting it.
func create(path string) (*box.Box, context.CancelFunc, error) {
	ctx := include.Context(service.ContextWith(context.Background(), deprecated.NewStderrManager(log.StdLogger())))
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, nil, fmt.Errorf("read config: %w", err)
	}
	options, err := json.UnmarshalExtendedContext[option.Options](ctx, content)
	if err != nil {
		return nil, nil, fmt.Errorf("decode config: %w", err)
	}
	// The client always reads the log through a pipe, never a terminal.
	if options.Log == nil {
		options.Log = &option.LogOptions{}
	}
	options.Log.DisableColor = true
	ctx, cancel := context.WithCancel(service.ExtendContext(ctx))
	instance, err := box.New(box.Options{Context: ctx, Options: options})
	if err != nil {
		cancel()
		return nil, nil, fmt.Errorf("create service: %w", err)
	}
	return instance, cancel, nil
}

func check(path string) error {
	instance, cancel, err := create(path)
	if err != nil {
		return err
	}
	cancel()
	return instance.Close()
}

// run serves until it is asked to stop, then closes the service so that system
// routes and DNS settings taken by a TUN inbound are given back before it exits.
func run(path string) error {
	instance, cancel, err := create(path)
	if err != nil {
		return err
	}
	// On Windows, Ctrl+Break arrives as os.Interrupt and a console close as SIGTERM.
	signals := make(chan os.Signal, 1)
	signal.Notify(signals, os.Interrupt, syscall.SIGTERM, syscall.SIGHUP)
	defer signal.Stop(signals)

	started := make(chan error, 1)
	go func() { started <- instance.Start() }()
	select {
	case err = <-started:
		if err != nil {
			cancel()
			_ = instance.Close()
			return fmt.Errorf("start service: %w", err)
		}
	case <-signals:
		cancel()
		return closeWithin(instance)
	}
	debug.FreeOSMemory()
	<-signals
	cancel()
	return closeWithin(instance)
}

// closeWithin bounds shutdown: a core that cannot close must not linger with the network taken.
func closeWithin(instance *box.Box) error {
	done := make(chan error, 1)
	go func() { done <- instance.Close() }()
	select {
	case err := <-done:
		return err
	case <-time.After(C.FatalStopTimeout):
		return errors.New("service did not close in time")
	}
}
