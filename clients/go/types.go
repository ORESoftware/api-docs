// Package oresapidocs implements the transport-neutral v1 RPC call/receipt envelope.
package oresapidocs

import (
	"encoding/json"
	"errors"
	"strconv"
	"sync/atomic"
	"unicode/utf8"
)

const (
	RPCVersion        uint8 = 1
	MaxFrameBytes           = 8 * 1024 * 1024
	LengthPrefixBytes       = 4
)

type Transport string

const (
	HTTP      Transport = "http"
	TCP       Transport = "tcp"
	WebSocket Transport = "websocket"
	NATS      Transport = "nats"
)

type OptionalJSON struct {
	Present bool
	Value   json.RawMessage
}

func SomeJSON(value json.RawMessage) OptionalJSON {
	return OptionalJSON{Present: true, Value: clone(value)}
}

type Call struct {
	Version   uint8
	ID        string
	Key       string
	Transport *Transport
	Path      OptionalJSON
	Query     OptionalJSON
	Body      OptionalJSON
	TraceID   *string
	SpanID    *string
}

type Receipt struct {
	Version   uint8
	ID        string
	Key       string
	Transport *Transport
	OK        bool
	Status    *uint16
	Body      OptionalJSON
	Error     OptionalJSON
	TraceID   *string
	SpanID    *string
}

func NewCall(id, key string) Call { return Call{Version: RPCVersion, ID: id, Key: key} }
func SuccessReceipt(id, key string, status uint16, body OptionalJSON) Receipt {
	return Receipt{Version: RPCVersion, ID: id, Key: key, OK: true, Status: &status, Body: body}
}
func ErrorReceipt(id, key string, status uint16, problem json.RawMessage) Receipt {
	return Receipt{Version: RPCVersion, ID: id, Key: key, OK: false, Status: &status, Error: SomeJSON(problem)}
}

type Correlator struct {
	prefix string
	next   atomic.Uint64
}

func NewCorrelator(prefix string) (*Correlator, error) {
	if !utf8.ValidString(prefix) || utf8.RuneCountInString(prefix) >= 128 {
		return nil, errors.New("correlation prefix must contain fewer than 128 characters")
	}
	return &Correlator{prefix: prefix}, nil
}
func (c *Correlator) Take() (string, error) {
	value := c.next.Add(1)
	if value == 0 {
		return "", errors.New("correlation id counter exhausted")
	}
	result := c.prefix + strconv.FormatUint(value, 10)
	if !validString(result, 128) {
		return "", errors.New("correlation id exceeds 128 characters")
	}
	return result, nil
}

func clone(value []byte) []byte {
	if value == nil {
		return nil
	}
	return append([]byte(nil), value...)
}
