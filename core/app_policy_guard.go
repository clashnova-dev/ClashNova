package main

import (
	"context"
	"github.com/sagernet/sing-box/adapter"
	"github.com/sagernet/sing-tun"
	N "github.com/sagernet/sing/common/network"
	"net"
)

// Traffic registers each connection before this guard. A connection routed just
// before an update is either in its close snapshot or sees the new block here.
type policyGuard struct{ state *policyState }

func (g *policyGuard) blocked(metadata adapter.InboundContext) bool {
	return metadata.ProcessInfo != nil && metadata.ProcessInfo.ProcessPath != "" && g.state.current.Load().policy(metadata.ProcessInfo.ProcessPath) == "block"
}
func (g *policyGuard) RoutedConnection(_ context.Context, conn net.Conn, metadata adapter.InboundContext, _ adapter.Rule, _ adapter.Outbound) net.Conn {
	if g.blocked(metadata) {
		_ = conn.Close()
	}
	return conn
}
func (g *policyGuard) RoutedPacketConnection(_ context.Context, conn N.PacketConn, metadata adapter.InboundContext, _ adapter.Rule, _ adapter.Outbound) N.PacketConn {
	if g.blocked(metadata) {
		_ = conn.Close()
	}
	return conn
}
func (g *policyGuard) RoutedFlow(_ context.Context, metadata adapter.InboundContext, _ adapter.Rule, _ adapter.Outbound) tun.FlowTracker {
	return &policyFlowGuard{guard: g, metadata: metadata}
}

type policyFlowGuard struct {
	guard    *policyGuard
	metadata adapter.InboundContext
}

func (g *policyFlowGuard) AttachFlow(handle tun.FlowHandle) {
	if g.guard.blocked(g.metadata) {
		handle.CloseFlow()
	}
}
func (*policyFlowGuard) CountForward(int)              {}
func (*policyFlowGuard) CountReverse(int)              {}
func (*policyFlowGuard) FlowEstablished()              {}
func (*policyFlowGuard) CloseFlow(tun.FlowCloseReason) {}
