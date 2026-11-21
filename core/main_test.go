package main

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeConfig(t *testing.T, content string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "config.json")
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

// The shape the client writes: a local inbound, a selector over nodes, and the traffic API.
const clientConfig = `{
  "log": {"level": "info", "timestamp": false},
  "inbounds": [{"type": "mixed", "tag": "mixed-in", "listen": "127.0.0.1", "listen_port": 29870}],
  "outbounds": [
    {"type": "direct", "tag": "direct"},
    {"type": "shadowsocks", "tag": "node", "server": "127.0.0.1", "server_port": 8388, "method": "aes-128-gcm", "password": "test"},
    {"type": "hysteria2", "tag": "quic-node", "server": "127.0.0.1", "server_port": 8443, "password": "test", "tls": {"enabled": true, "server_name": "example.com"}},
    {"type": "selector", "tag": "select", "outbounds": ["node", "quic-node"]}
  ],
  "route": {"final": "select", "rules": [{"ip_is_private": true, "outbound": "direct"}]},
  "experimental": {"clash_api": {"external_controller": "127.0.0.1:29869", "secret": "test"}}
}`

func TestCheckAcceptsClientConfig(t *testing.T) {
	if err := execute([]string{"check", "-c", writeConfig(t, clientConfig)}, new(bytes.Buffer)); err != nil {
		t.Fatal(err)
	}
}

func TestCheckRejectsWhatTheCoreCannotRun(t *testing.T) {
	for name, content := range map[string]string{
		"truncated":     `{"outbounds":[`,
		"unknown field": `{"outbounds":[{"type":"direct","tag":"direct","no_such_field":1}]}`,
		// naive is left out of this build; see CORE_TAGS in scripts/prepare-sidecars.py.
		"left out": `{"outbounds":[{"type":"naive","tag":"node","server":"example.com","server_port":443}]}`,
	} {
		if err := execute([]string{"check", "-c", writeConfig(t, content)}, new(bytes.Buffer)); err == nil {
			t.Errorf("%s: config was accepted", name)
		}
	}
	if err := execute([]string{"check", "-c", filepath.Join(t.TempDir(), "absent.json")}, new(bytes.Buffer)); err == nil {
		t.Error("missing file was accepted")
	}
}

func TestCommandLine(t *testing.T) {
	for _, args := range [][]string{nil, {"run"}, {"check"}, {"check", "-c"}, {"check", "config.json"}, {"version", "extra"}, {"api"}} {
		if err := execute(args, new(bytes.Buffer)); err == nil || err.Error() != usage {
			t.Errorf("%v: expected usage, got %v", args, err)
		}
	}
	var output bytes.Buffer
	if err := execute([]string{"version"}, &output); err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(output.String(), "airport-core (sing-box ") {
		t.Errorf("unexpected version output: %q", output.String())
	}
}
