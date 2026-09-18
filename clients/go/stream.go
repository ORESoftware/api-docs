package oresapidocs

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
)

var ErrRPCStreamAlreadyOpened = errors.New("an RPC stream call builder can only be opened once")

type RPCStreamCarrier string

const (
	RPCStreamWebSocket RPCStreamCarrier = "websocket"
	RPCStreamTCP       RPCStreamCarrier = "tcp"
)

type RPCStreamFrameKind string

const (
	RPCStreamData   RPCStreamFrameKind = "data"
	RPCStreamEnd    RPCStreamFrameKind = "end"
	RPCStreamError  RPCStreamFrameKind = "error"
	RPCStreamCancel RPCStreamFrameKind = "cancel"
	RPCStreamCall   RPCStreamFrameKind = "call"
)

type RPCStreamFrame struct {
	ID      string
	Kind    RPCStreamFrameKind
	Body    json.RawMessage
	HasBody bool
	Code    string
	Message string
}

type RPCStreamCallFrame struct {
	ID     string
	Key    string
	Method string
	Path   string
	Query  [][2]string
	Body   json.RawMessage
}

type RPCStreamSession interface {
	Next(context.Context) (RPCStreamFrame, bool, error)
	Cancel(context.Context) error
}

type FramedRPCStream interface {
	Carrier() RPCStreamCarrier
	Open(context.Context, RPCStreamCallFrame) (RPCStreamSession, error)
}

type RPCStreamRequest struct {
	Method string
	Path   string
	Query  [][2]string
	Body   json.RawMessage
}

type RPCStreamContext struct {
	ID        string
	Key       string
	Carrier   RPCStreamCarrier
	Ended     bool
	Cancelled bool
	Err       error
}

type RPCStreamRemoteError struct {
	Code    string
	Message string
}

func (e *RPCStreamRemoteError) Error() string {
	if e.Message == "" {
		return fmt.Sprintf("remote RPC stream error %s", e.Code)
	}
	return fmt.Sprintf("remote RPC stream error %s: %s", e.Code, e.Message)
}

type RPCStreamClient[T any] struct {
	session   RPCStreamSession
	id        string
	key       string
	carrier   RPCStreamCarrier
	decode    func(json.RawMessage) (T, error)
	ended     bool
	cancelled bool
	done      bool
	err       error
}

func (c *RPCStreamClient[T]) Context() RPCStreamContext {
	return RPCStreamContext{
		ID: c.id, Key: c.key, Carrier: c.carrier,
		Ended: c.ended, Cancelled: c.cancelled, Err: c.err,
	}
}

func (c *RPCStreamClient[T]) Cancel(ctx context.Context) error {
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

func (c *RPCStreamClient[T]) Next(ctx context.Context) (T, bool, error) {
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
	case RPCStreamData:
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
	case RPCStreamEnd:
		c.ended = true
		c.done = true
		return zero, false, nil
	case RPCStreamError:
		c.done = true
		c.err = &RPCStreamRemoteError{Code: frame.Code, Message: frame.Message}
		return zero, false, c.err
	case RPCStreamCancel:
		c.cancelled = true
		c.done = true
		return zero, false, nil
	case RPCStreamCall:
		c.done = true
		c.err = errors.New("a call frame cannot arrive inside its response stream")
		return zero, false, c.err
	default:
		c.done = true
		c.err = fmt.Errorf("unsupported stream frame kind %q", frame.Kind)
		return zero, false, c.err
	}
}

type RPCStreamCallBuilder[T any] struct {
	owner       *OresRPCStreamClient
	key         string
	request     RPCStreamRequest
	decode      func(json.RawMessage) (T, error)
	protocolErr error
	opened      bool
}

func (b *RPCStreamCallBuilder[T]) AddQueryField(name string, value any) *RPCStreamCallBuilder[T] {
	b.request.Query = append(b.request.Query, [2]string{name, fmt.Sprint(value)})
	return b
}

func (b *RPCStreamCallBuilder[T]) WithBody(value any) *RPCStreamCallBuilder[T] {
	raw, err := json.Marshal(value)
	if err != nil {
		b.protocolErr = err
		return b
	}
	b.request.Body = raw
	return b
}

func (b *RPCStreamCallBuilder[T]) AddBodyField(name string, value any) *RPCStreamCallBuilder[T] {
	if b.protocolErr != nil {
		return b
	}
	body := map[string]any{}
	if len(b.request.Body) != 0 {
		if err := json.Unmarshal(b.request.Body, &body); err != nil {
			b.protocolErr = errors.New("AddBodyField requires an object RPC body")
			return b
		}
	}
	body[name] = value
	raw, err := json.Marshal(body)
	if err != nil {
		b.protocolErr = err
		return b
	}
	b.request.Body = raw
	return b
}

// Stream is the sole transport-open / network-I/O boundary.
func (b *RPCStreamCallBuilder[T]) Stream(ctx context.Context) (*RPCStreamClient[T], error) {
	if b.opened {
		return nil, ErrRPCStreamAlreadyOpened
	}
	b.opened = true
	if b.protocolErr != nil {
		return nil, b.protocolErr
	}
	id, err := b.owner.correlator.Take()
	if err != nil {
		return nil, err
	}
	call := RPCStreamCallFrame{
		ID: id, Key: b.key, Method: b.request.Method, Path: b.request.Path,
		Query: append([][2]string(nil), b.request.Query...),
		Body:  append(json.RawMessage(nil), b.request.Body...),
	}
	session, err := b.owner.stream.Open(ctx, call)
	if err != nil {
		return nil, fmt.Errorf("%s: opening stream failed: %w", b.key, err)
	}
	return &RPCStreamClient[T]{
		session: session, id: id, key: b.key,
		carrier: b.owner.stream.Carrier(), decode: b.decode,
	}, nil
}

type OresRPCStreamClient struct {
	stream     FramedRPCStream
	operations map[string]struct{}
	correlator *Correlator
}

func NewOresRPCStreamClient(
	stream FramedRPCStream,
	operations []string,
	idPrefix string,
) (*OresRPCStreamClient, error) {
	if stream == nil {
		return nil, errors.New("RPC framed stream is required")
	}
	carrier := stream.Carrier()
	if carrier != RPCStreamWebSocket && carrier != RPCStreamTCP {
		return nil, fmt.Errorf("RPC framed stream carrier must be websocket or tcp, got %q", carrier)
	}
	correlator, err := NewCorrelator(idPrefix)
	if err != nil {
		return nil, err
	}
	allowed := make(map[string]struct{}, len(operations))
	for _, key := range operations {
		allowed[key] = struct{}{}
	}
	return &OresRPCStreamClient{stream: stream, operations: allowed, correlator: correlator}, nil
}

func PrepareRPCStream[T any](
	client *OresRPCStreamClient,
	key string,
	request RPCStreamRequest,
	decode func(json.RawMessage) (T, error),
) (*RPCStreamCallBuilder[T], error) {
	if client == nil {
		return nil, errors.New("RPC stream client is required")
	}
	if _, ok := client.operations[key]; !ok {
		return nil, fmt.Errorf("RPC stream operation not generated for this audience: %s", key)
	}
	if request.Method == "" || request.Path == "" {
		return nil, errors.New("RPC stream request requires method and path")
	}
	if decode == nil {
		return nil, errors.New("RPC stream decoder is required")
	}
	return &RPCStreamCallBuilder[T]{owner: client, key: key, request: request, decode: decode}, nil
}
