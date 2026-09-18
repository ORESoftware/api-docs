package ridlruntime

import (
	"context"
	"encoding/json"
	"errors"
	"testing"
)

type cannedSession struct {
	frames  []Frame
	index   int
	cancels int
	err     error
}

func (s *cannedSession) Next(context.Context) (Frame, bool, error) {
	if s.err != nil {
		err := s.err
		s.err = nil
		return Frame{}, false, err
	}
	if s.index >= len(s.frames) {
		return Frame{}, false, nil
	}
	frame := s.frames[s.index]
	s.index++
	return frame, true, nil
}

func (s *cannedSession) Cancel(context.Context) error {
	s.cancels++
	return nil
}

type cannedStream struct {
	carrier string
	session *cannedSession
	opens   int
}

func (s *cannedStream) Carrier() string { return s.carrier }

func (s *cannedStream) Open(_ context.Context, _ Frame) (StreamSession, error) {
	s.opens++
	return s.session, nil
}

func intDecoder(body json.RawMessage) (int, error) {
	var value int
	if err := json.Unmarshal(body, &value); err != nil {
		return 0, err
	}
	return value, nil
}

func TestStreamBuilderIsDeferredUntilStream(t *testing.T) {
	session := &cannedSession{frames: []Frame{
		DataFrame("s-1", json.RawMessage("1")),
		DataFrame("s-1", json.RawMessage("2")),
		EndFrame("s-1"),
	}}
	wire := &cannedStream{carrier: "websocket", session: session}
	transport := NewFramedStreamTransport(wire, "s-")
	builder := PrepareStream(transport, StreamRequest{
		Key: "demo.events.watch_stream", Method: "GET", Path: "/v1/events",
	}, intDecoder)

	if wire.opens != 0 {
		t.Fatalf("building a stream performed I/O: opens=%d", wire.opens)
	}
	client, err := builder.Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if wire.opens != 1 {
		t.Fatalf("stream() did not perform exactly one open: %d", wire.opens)
	}

	first, ok, err := client.Next(context.Background())
	if err != nil || !ok || first != 1 {
		t.Fatalf("first=%d ok=%v err=%v", first, ok, err)
	}
	second, ok, err := client.Next(context.Background())
	if err != nil || !ok || second != 2 {
		t.Fatalf("second=%d ok=%v err=%v", second, ok, err)
	}
	_, ok, err = client.Next(context.Background())
	if err != nil || ok {
		t.Fatalf("terminal next ok=%v err=%v", ok, err)
	}
	if !client.Context().Ended {
		t.Fatal("stream did not record end")
	}
}

func TestStreamBuilderCannotOpenTwice(t *testing.T) {
	wire := &cannedStream{
		carrier: "tcp",
		session: &cannedSession{frames: []Frame{EndFrame("once-1")}},
	}
	builder := PrepareStream(
		NewFramedStreamTransport(wire, "once-"),
		StreamRequest{Key: "demo.watch_stream", Method: "GET", Path: "/v1/watch"},
		intDecoder,
	)
	if _, err := builder.Stream(context.Background()); err != nil {
		t.Fatal(err)
	}
	if _, err := builder.Stream(context.Background()); !errors.Is(err, ErrStreamAlreadyOpened) {
		t.Fatalf("second open error=%v", err)
	}
}

func TestStreamClientRejectsCorrelationMismatch(t *testing.T) {
	wire := &cannedStream{
		carrier: "websocket",
		session: &cannedSession{frames: []Frame{
			DataFrame("wrong", json.RawMessage("1")),
		}},
	}
	client, err := PrepareStream(
		NewFramedStreamTransport(wire, "expected-"),
		StreamRequest{Key: "demo.watch_stream", Method: "GET", Path: "/v1/watch"},
		intDecoder,
	).Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if _, _, err := client.Next(context.Background()); err == nil {
		t.Fatal("correlation mismatch was accepted")
	}
}

func TestStreamCancellationIsDelegatedOnce(t *testing.T) {
	session := &cannedSession{}
	wire := &cannedStream{carrier: "tcp", session: session}
	client, err := PrepareStream(
		NewFramedStreamTransport(wire, "cancel-"),
		StreamRequest{Key: "demo.watch_stream", Method: "GET", Path: "/v1/watch"},
		intDecoder,
	).Stream(context.Background())
	if err != nil {
		t.Fatal(err)
	}
	if err := client.Cancel(context.Background()); err != nil {
		t.Fatal(err)
	}
	if err := client.Cancel(context.Background()); err != nil {
		t.Fatal(err)
	}
	if session.cancels != 1 {
		t.Fatalf("cancel count=%d", session.cancels)
	}
	if !client.Context().Cancelled {
		t.Fatal("stream did not record cancellation")
	}
}
