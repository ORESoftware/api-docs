// Package ridlruntime implements the transport-neutral RIDL frame envelope.
//
// It is a standard-library-only port of ridl/framing.py. HTTP does not use this
// envelope; WebSocket carries one encoded frame per text message and TCP uses a
// four-byte big-endian length prefix followed by the same UTF-8 JSON bytes.
package ridlruntime

import (
	"bytes"
	"encoding/binary"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"sync/atomic"
	"unicode/utf8"
)

const (
	FrameVersion      uint8 = 1
	MaxFrameBytes           = 8 * 1024 * 1024
	LengthPrefixBytes       = 4
)

type FrameKind string

const (
	Call   FrameKind = "call"
	Data   FrameKind = "data"
	End    FrameKind = "end"
	Error  FrameKind = "error"
	Cancel FrameKind = "cancel"
)

var allowedFields = map[string]struct{}{
	"v": {}, "id": {}, "t": {}, "key": {}, "method": {}, "path": {},
	"query": {}, "body": {}, "code": {}, "message": {}, "meta": {},
}

type QueryPair [2]string

// Frame distinguishes an absent body (HasBody=false) from a JSON null body
// (HasBody=true and Body == "null"). Body is raw JSON so object member order is
// preserved across decode/encode; Encode compacts it and rejects invalid JSON.
type Frame struct {
	Version uint8
	ID      string
	Kind    FrameKind
	Key     string
	Method  string
	Path    string
	Query   []QueryPair
	Body    json.RawMessage
	HasBody bool
	Code    string
	Message *string
	Meta    map[string]string
}

func CallFrame(id, key, method, path string, query []QueryPair, body json.RawMessage) Frame {
	return Frame{
		Version: FrameVersion,
		ID:      id,
		Kind:    Call,
		Key:     key,
		Method:  method,
		Path:    path,
		Query:   append([]QueryPair(nil), query...),
		Body:    cloneBytes(body),
		HasBody: body != nil,
		Meta:    map[string]string{},
	}
}

func DataFrame(id string, body json.RawMessage) Frame {
	return Frame{Version: FrameVersion, ID: id, Kind: Data, Body: cloneBytes(body), HasBody: true, Meta: map[string]string{}}
}

func EndFrame(id string) Frame {
	return Frame{Version: FrameVersion, ID: id, Kind: End, Meta: map[string]string{}}
}

func CancelFrame(id string) Frame {
	return Frame{Version: FrameVersion, ID: id, Kind: Cancel, Meta: map[string]string{}}
}

func ErrorFrame(id, code string, message *string, body json.RawMessage) Frame {
	return Frame{Version: FrameVersion, ID: id, Kind: Error, Code: code, Message: message, Body: cloneBytes(body), HasBody: body != nil, Meta: map[string]string{}}
}

func (f Frame) WithMeta(name, value string) Frame {
	copied := make(map[string]string, len(f.Meta)+1)
	for key, item := range f.Meta {
		copied[key] = item
	}
	copied[name] = value
	f.Meta = copied
	return f
}

func (f Frame) Validate() error {
	if f.Version != FrameVersion {
		return fmt.Errorf("unsupported frame version %d", f.Version)
	}
	if !utf8.ValidString(f.ID) || f.ID == "" || utf8.RuneCountInString(f.ID) > 128 {
		return errors.New("id must be 1..128 characters")
	}
	switch f.Kind {
	case Call, Data, End, Error, Cancel:
	default:
		return fmt.Errorf("unknown frame type %q", f.Kind)
	}
	if f.Kind == Call {
		if f.Key == "" {
			return errors.New("a call frame needs an operation key")
		}
		if f.Method == "" {
			return errors.New("a call frame needs a method")
		}
		if !strings.HasPrefix(f.Path, "/") {
			return errors.New("a call frame needs a path starting with /")
		}
	} else if f.Key != "" || f.Method != "" || f.Path != "" || len(f.Query) != 0 {
		return fmt.Errorf("a %s frame carries no addressing fields", f.Kind)
	}
	if f.Kind == Data && !f.HasBody {
		return errors.New("a data frame needs a body")
	}
	if f.Kind == Error {
		if f.Code == "" {
			return errors.New("an error frame needs a code")
		}
	} else if f.Code != "" || f.Message != nil {
		return fmt.Errorf("a %s frame carries no code or message", f.Kind)
	}
	if f.HasBody {
		if len(f.Body) == 0 || !json.Valid(f.Body) {
			return errors.New("body must contain one valid JSON value")
		}
	} else if len(f.Body) != 0 {
		return errors.New("body bytes are present while HasBody is false")
	}
	for index, pair := range f.Query {
		if !utf8.ValidString(pair[0]) || !utf8.ValidString(pair[1]) {
			return fmt.Errorf("query[%d] must contain UTF-8 strings", index)
		}
	}
	for key, value := range f.Meta {
		if !utf8.ValidString(key) || !utf8.ValidString(value) {
			return fmt.Errorf("meta.%s must be a UTF-8 string", key)
		}
	}
	return nil
}

func (f Frame) Encode() ([]byte, error) {
	if err := f.Validate(); err != nil {
		return nil, err
	}
	var out bytes.Buffer
	out.Grow(256)
	out.WriteString(`{"v":`)
	out.WriteString(strconv.Itoa(int(f.Version)))
	out.WriteString(`,"id":`)
	if err := writeJSONString(&out, f.ID); err != nil {
		return nil, err
	}
	out.WriteString(`,"t":`)
	if err := writeJSONString(&out, string(f.Kind)); err != nil {
		return nil, err
	}
	if f.Kind == Call {
		out.WriteString(`,"key":`)
		if err := writeJSONString(&out, f.Key); err != nil {
			return nil, err
		}
		out.WriteString(`,"method":`)
		if err := writeJSONString(&out, f.Method); err != nil {
			return nil, err
		}
		out.WriteString(`,"path":`)
		if err := writeJSONString(&out, f.Path); err != nil {
			return nil, err
		}
		if len(f.Query) > 0 {
			out.WriteString(`,"query":[`)
			for index, pair := range f.Query {
				if index > 0 {
					out.WriteByte(',')
				}
				out.WriteByte('[')
				if err := writeJSONString(&out, pair[0]); err != nil {
					return nil, err
				}
				out.WriteByte(',')
				if err := writeJSONString(&out, pair[1]); err != nil {
					return nil, err
				}
				out.WriteByte(']')
			}
			out.WriteByte(']')
		}
	}
	if f.HasBody {
		out.WriteString(`,"body":`)
		if err := compactJSON(&out, f.Body); err != nil {
			return nil, err
		}
	}
	if f.Kind == Error {
		out.WriteString(`,"code":`)
		if err := writeJSONString(&out, f.Code); err != nil {
			return nil, err
		}
		if f.Message != nil {
			out.WriteString(`,"message":`)
			if err := writeJSONString(&out, *f.Message); err != nil {
				return nil, err
			}
		}
	}
	if len(f.Meta) > 0 {
		keys := make([]string, 0, len(f.Meta))
		for key := range f.Meta {
			keys = append(keys, key)
		}
		sort.Strings(keys)
		out.WriteString(`,"meta":{`)
		for index, key := range keys {
			if index > 0 {
				out.WriteByte(',')
			}
			if err := writeJSONString(&out, key); err != nil {
				return nil, err
			}
			out.WriteByte(':')
			if err := writeJSONString(&out, f.Meta[key]); err != nil {
				return nil, err
			}
		}
		out.WriteByte('}')
	}
	out.WriteByte('}')
	if out.Len() > MaxFrameBytes {
		return nil, fmt.Errorf("frame is %d bytes, over the %d limit", out.Len(), MaxFrameBytes)
	}
	return out.Bytes(), nil
}

func (f Frame) EncodeTCP() ([]byte, error) {
	payload, err := f.Encode()
	if err != nil {
		return nil, err
	}
	out := make([]byte, LengthPrefixBytes+len(payload))
	binary.BigEndian.PutUint32(out[:LengthPrefixBytes], uint32(len(payload)))
	copy(out[LengthPrefixBytes:], payload)
	return out, nil
}

func Decode(payload []byte) (Frame, error) {
	if len(payload) > MaxFrameBytes {
		return Frame{}, fmt.Errorf("frame is %d bytes, over the %d limit", len(payload), MaxFrameBytes)
	}
	if !utf8.Valid(payload) {
		return Frame{}, errors.New("frame is not UTF-8")
	}
	var raw map[string]json.RawMessage
	if err := json.Unmarshal(payload, &raw); err != nil {
		return Frame{}, fmt.Errorf("frame is not JSON: %w", err)
	}
	if raw == nil {
		return Frame{}, errors.New("a frame must be a JSON object")
	}
	unknown := make([]string, 0)
	for key := range raw {
		if _, ok := allowedFields[key]; !ok {
			unknown = append(unknown, key)
		}
	}
	if len(unknown) > 0 {
		sort.Strings(unknown)
		return Frame{}, fmt.Errorf("unknown frame member(s): %s", strings.Join(unknown, ", "))
	}

	var frame Frame
	frame.Meta = map[string]string{}
	if err := required(raw, "v", &frame.Version); err != nil {
		return Frame{}, err
	}
	if err := required(raw, "id", &frame.ID); err != nil {
		return Frame{}, err
	}
	var kind string
	if err := required(raw, "t", &kind); err != nil {
		return Frame{}, err
	}
	frame.Kind = FrameKind(kind)
	if err := optional(raw, "key", &frame.Key); err != nil {
		return Frame{}, err
	}
	if err := optional(raw, "method", &frame.Method); err != nil {
		return Frame{}, err
	}
	if err := optional(raw, "path", &frame.Path); err != nil {
		return Frame{}, err
	}
	if value, ok := raw["query"]; ok {
		var pairs [][]string
		if err := json.Unmarshal(value, &pairs); err != nil {
			return Frame{}, errors.New("query must be an array of [name, value] pairs")
		}
		for _, pair := range pairs {
			if len(pair) != 2 {
				return Frame{}, errors.New("each query entry must be a [name, value] pair of strings")
			}
			frame.Query = append(frame.Query, QueryPair{pair[0], pair[1]})
		}
	}
	if value, ok := raw["body"]; ok {
		frame.HasBody = true
		frame.Body = cloneBytes(value)
	}
	if err := optional(raw, "code", &frame.Code); err != nil {
		return Frame{}, err
	}
	if value, ok := raw["message"]; ok {
		var message string
		if err := json.Unmarshal(value, &message); err != nil {
			return Frame{}, errors.New("message must be a string")
		}
		frame.Message = &message
	}
	if value, ok := raw["meta"]; ok {
		if err := json.Unmarshal(value, &frame.Meta); err != nil {
			return Frame{}, errors.New("meta must be an object of strings")
		}
		if frame.Meta == nil {
			return Frame{}, errors.New("meta must be an object of strings")
		}
	}
	if err := frame.Validate(); err != nil {
		return Frame{}, err
	}
	return frame, nil
}

func DecodeStream(buffer []byte) ([]Frame, []byte, error) {
	frames := make([]Frame, 0)
	offset := 0
	for len(buffer)-offset >= LengthPrefixBytes {
		length := int(binary.BigEndian.Uint32(buffer[offset : offset+LengthPrefixBytes]))
		if length > MaxFrameBytes {
			return nil, nil, fmt.Errorf("declared frame length %d is over the %d limit", length, MaxFrameBytes)
		}
		start := offset + LengthPrefixBytes
		if len(buffer)-start < length {
			break
		}
		frame, err := Decode(buffer[start : start+length])
		if err != nil {
			return nil, nil, err
		}
		frames = append(frames, frame)
		offset = start + length
	}
	return frames, buffer[offset:], nil
}

type Correlator struct {
	prefix string
	next   atomic.Uint64
}

func NewCorrelator(prefix string) *Correlator { return &Correlator{prefix: prefix} }

func (c *Correlator) Take() string {
	value := c.next.Add(1)
	return c.prefix + strconv.FormatUint(value, 10)
}

func writeJSONString(out *bytes.Buffer, value string) error {
	if !utf8.ValidString(value) {
		return errors.New("string is not UTF-8")
	}
	var encoded bytes.Buffer
	encoder := json.NewEncoder(&encoded)
	encoder.SetEscapeHTML(false)
	if err := encoder.Encode(value); err != nil {
		return err
	}
	bytesValue := encoded.Bytes()
	out.Write(bytesValue[:len(bytesValue)-1])
	return nil
}

func compactJSON(out *bytes.Buffer, value json.RawMessage) error {
	if !json.Valid(value) {
		return errors.New("body must contain one valid JSON value")
	}
	var compacted bytes.Buffer
	if err := json.Compact(&compacted, value); err != nil {
		return err
	}
	out.Write(compacted.Bytes())
	return nil
}

func required(raw map[string]json.RawMessage, name string, target any) error {
	value, ok := raw[name]
	if !ok {
		return fmt.Errorf("missing frame member %s", name)
	}
	if err := json.Unmarshal(value, target); err != nil {
		return fmt.Errorf("%s has the wrong type", name)
	}
	return nil
}

func optional(raw map[string]json.RawMessage, name string, target any) error {
	value, ok := raw[name]
	if !ok {
		return nil
	}
	if err := json.Unmarshal(value, target); err != nil {
		return fmt.Errorf("%s has the wrong type", name)
	}
	return nil
}

func cloneBytes(value []byte) []byte {
	if value == nil {
		return nil
	}
	return append([]byte(nil), value...)
}
