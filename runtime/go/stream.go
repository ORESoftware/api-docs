package ridlruntime

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sync/atomic"
)

var ErrStreamAlreadyOpened = errors.New("an RPC stream call builder can only be opened once")

type StreamSession interface {
	Next(context.Context) (Frame, bool, error)
	Cancel(context.Context) error
}

type FramedStream interface {
	Carrier() string
	Open(context.Context, Frame) (StreamSession, error)
}

type StreamRequest struct {
	Key    string
	Method string
	Path   string
	Query  []QueryPair
	Body   json.RawMessage
}

type RpcStreamContext struct {
	ID        string
	Key       string
	Carrier   string
	Ended     bool
	Cancelled bool
	Err       error
}

type StreamRemoteError struct {
	Code    string
	Message string
}

func (e *StreamRemoteError) Error() string {
	if e.Message == "" {
		return fmt.Sprintf("remote RPC stream error %s", e.Code)
	}
	return fmt.Sprintf("remote RPC stream error %s: %s", e.Code, e.Message)
}

type RpcStreamClient[T any] struct {
	session   StreamSession
	id        string
	key       string
	carrier   string
	decode    func(json.RawMessage) (T, error)
	ended     bool
	cancelled bool
	done      bool
	err       error
}

func (c *RpcStreamClient[T]) Context() RpcStreamContext {
	return RpcStreamContext{
		ID:        c.id,
		Key:       c.key,
		Carrier:   c.carrier,
		Ended:     c.ended,
		Cancelled: c.cancelled,
		Err:       c.err,
	}
}

func (c *RpcStreamClient[T]) Cancel(ctx context.Context) error {
	if c.ended || c.cancelled {
		return nil
	}
	if err := c.session.Cancel(ctx); err != nil {
		c.err = fmt.Errorf("%s: stream cancellation failed: %w", c.key, err)
		return c.err
	}
	c.cancelled = true
	c.done = true
	return nil
}

func (c *RpcStreamClient[T]) Next(ctx context.Context) (T, bool, error) {
	var zero T
	if c.done {
		return zero, false, nil
	}
	frame, ok, err := c.session.Next(ctx)
	if err != nil {
		c.done = true
		c.err = fmt.Errorf("%s: stream transport failed: %w", c.key, err)
		return zero, false, c.err
	}
	if !ok {
		c.done = true
		if c.ended || c.cancelled {
			return zero, false, nil
		}
		c.err = errors.New("stream transport ended without an end or cancel frame")
		return zero, false, c.err
	}
	if frame.ID != c.id {
		c.done = true
		c.err = fmt.Errorf("frame for correlation id %s arrived on the stream for %s", frame.ID, c.id)
		return zero, false, c.err
	}

	switch frame.Kind {
	case Data:
		if !frame.HasBody {
			c.done = true
			c.err = errors.New("a stream data frame arrived without a body")
			return zero, false, c.err
		}
		value, err := c.decode(frame.Body)
		if err != nil {
			c.done = true
			c.err = fmt.Errorf("%s: stream data failed typed decoding: %w", c.key, err)
			return zero, false, c.err
		}
		return value, true, nil
	case End:
		c.ended = true
		c.done = true
		return zero, false, nil
	case Error:
		c.done = true
		message := ""
		if frame.Message != nil {
			message = *frame.Message
		}
		c.err = &StreamRemoteError{Code: frame.Code, Message: message}
		return zero, false, c.err
	case Cancel:
		c.cancelled = true
		c.done = true
		return zero, false, nil
	case Call:
		c.done = true
		c.err = errors.New("a call frame cannot arrive inside its response stream")
		return zero, false, c.err
	default:
		c.done = true
		c.err = fmt.Errorf("unsupported stream frame kind %q", frame.Kind)
		return zero, false, c.err
	}
}

type RpcStreamCallBuilder[T any] struct {
	stream     FramedStream
	correlator *Correlator
	request    StreamRequest
	decode     func(json.RawMessage) (T, error)
	opened     atomic.Bool
}

func NewRpcStreamCallBuilder[T any](
	stream FramedStream,
	correlator *Correlator,
	request StreamRequest,
	decode func(json.RawMessage) (T, error),
) *RpcStreamCallBuilder[T] {
	return &RpcStreamCallBuilder[T]{
		stream:     stream,
		correlator: correlator,
		request:    request,
		decode:     decode,
	}
}

// Stream is the sole I/O boundary. Constructing and configuring the builder
// cannot open the transport.
func (b *RpcStreamCallBuilder[T]) Stream(ctx context.Context) (*RpcStreamClient[T], error) {
	if !b.opened.CompareAndSwap(false, true) {
		return nil, ErrStreamAlreadyOpened
	}
	carrier := b.stream.Carrier()
	if carrier != "websocket" && carrier != "tcp" {
		return nil, fmt.Errorf("stream carrier must be websocket or tcp, got %q", carrier)
	}
	id := b.correlator.Take()
	call := CallFrame(
		id,
		b.request.Key,
		b.request.Method,
		b.request.Path,
		b.request.Query,
		b.request.Body,
	)
	session, err := b.stream.Open(ctx, call)
	if err != nil {
		return nil, fmt.Errorf("%s: opening stream failed: %w", b.request.Key, err)
	}
	return &RpcStreamClient[T]{
		session: session,
		id:      id,
		key:     b.request.Key,
		carrier: carrier,
		decode:  b.decode,
	}, nil
}

type FramedStreamTransport struct {
	stream     FramedStream
	correlator *Correlator
}

func NewFramedStreamTransport(stream FramedStream, idPrefix string) *FramedStreamTransport {
	return &FramedStreamTransport{
		stream:     stream,
		correlator: NewCorrelator(idPrefix),
	}
}

func PrepareStream[T any](
	transport *FramedStreamTransport,
	request StreamRequest,
	decode func(json.RawMessage) (T, error),
) *RpcStreamCallBuilder[T] {
	return NewRpcStreamCallBuilder(transport.stream, transport.correlator, request, decode)
}
