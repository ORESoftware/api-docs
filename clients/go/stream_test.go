package oresapidocs

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
)

type testRPCStreamSession struct {
	frames  []RPCStreamFrame
	index   int
	cancels int
}

func (s *testRPCStreamSession) Next(context.Context) (RPCStreamFrame, bool, error) {
	if s.index >= len(s.frames) {
		return RPCStreamFrame{}, false, nil
	}
	frame := s.frames[s.index]
	s.index++
	return frame, true, nil
}

func (s *testRPCStreamSession) Cancel(context.Context) error {
	s.cancels++
	return nil
}

type testFramedRPCStream struct {
	carrier RPCStreamCarrier
	session *testRPCStreamSession
	opens   int
}

func (s *testFramedRPCStream) Carrier() RPCStreamCarrier { return s.carrier }

func (s *testFramedRPCStream) Open(
	_ context.Context,
	call RPCStreamCallFrame,
) (RPCStreamSession, error) {
	s.opens++
	for index := range s.session.frames {
		s.session.frames[index].ID = call.ID
	}
	return s.session, nil
}

func decodeStreamInt(body json.RawMessage) (int, error) {
	var value map[string]int
	if err := json.Unmarshal(body, &value); err != nil {
		return 0, err
	}
	return value["value"], nil
}

func TestRPCStreamPrepareIsInertUntilStream(t *testing.T) {
	session := &testRPCStreamSession{frames: []RPCStreamFrame{
		{Kind: RPCStreamData, Body: json.RawMessage("{\"value\":1}"), HasBody: true},
		{Kind: RPCStreamData, Body: json.RawMessage("{\"value\":2}"), HasBody: true},
		{Kind: RPCStreamEnd},
	}}
	wire := &testFramedRPCStream{carrier: RPCStreamWebSocket, session: session}
	client, err := NewOresRPCStreamClient(
		wire, []string{"demo.events.watch_stream"}, "s-",
	)
	if err != nil {
		t.Fatal(err)
	}
	builder, err := PrepareRPCStream(
		client,
		"demo.events.watch_stream",
		RPCStreamRequest{Method: "GET", Path: "/v1/events"},
		decodeStreamInt,
	)
	if err != nil {
		t.Fatal(err)
	}
	builder.AddQueryField("room_id", 7).WithBody(map[string]any{"active": true})
	if wire.opens != 0 {
		t.Fatalf("prepare/configure performed I/O: opens=%d", wire.opens)
	}
	stream, err := builder.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if wire.opens != 1 {
		t.Fatalf("Stream() did not perform exactly one open: %d", wire.opens)
	}
	first, ok, err := stream.Next(context.Background())
	if err != nil || !ok || first != 1 {
		t.Fatalf("first=%d ok=%v err=%v", first, ok, err)
	}
	second, ok, err := stream.Next(context.Background())
	if err != nil || !ok || second != 2 {
		t.Fatalf("second=%d ok=%v err=%v", second, ok, err)
	}
	_, ok, err = stream.Next(context.Background())
	if err != nil || ok {
		t.Fatalf("terminal next ok=%v err=%v", ok, err)
	}
	if !stream.Context().Ended {
		t.Fatal("stream did not record end")
	}
}

func TestRPCStreamBuilderCannotOpenTwice(t *testing.T) {
	wire := &testFramedRPCStream{
		carrier: RPCStreamTCP,
		session: &testRPCStreamSession{frames: []RPCStreamFrame{{Kind: RPCStreamEnd}}},
	}
	client, err := NewOresRPCStreamClient(wire, []string{"demo.watch_stream"}, "once-")
	if err != nil {
		t.Fatal(err)
	}
	builder, err := PrepareRPCStream(
		client,
		"demo.watch_stream",
		RPCStreamRequest{Method: "GET", Path: "/v1/watch"},
		decodeStreamInt,
	)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := builder.Stream(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, err := builder.Stream(context.Background()); !errors.Is(err, ErrRPCStreamAlreadyOpened) {
		t.Fatalf("second open error=%v", err)
	}
}

func TestRPCStreamCancellationDelegatesOnce(t *testing.T) {
	session := &testRPCStreamSession{}
	wire := &testFramedRPCStream{carrier: RPCStreamTCP, session: session}
	client, err := NewOresRPCStreamClient(wire, []string{"demo.watch_stream"}, "cancel-")
	if err != nil {
		t.Fatal(err)
	}
	builder, err := PrepareRPCStream(
		client,
		"demo.watch_stream",
		RPCStreamRequest{Method: "GET", Path: "/v1/watch"},
		decodeStreamInt,
	)
	if err != nil {
		t.Fatal(err)
	}
	stream, err := builder.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if err := stream.Cancel(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := stream.Cancel(context.Background()); err != nil {
		t.Fatal(err)
	}
	if session.cancels != 1 {
		t.Fatalf("cancel count=%d", session.cancels)
	}
}
