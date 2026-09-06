package oresapidocs

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strconv"
)

func (call Call) Encode() ([]byte, error) {
	if err := call.Validate(); err != nil {
		return nil, err
	}
	var out bytes.Buffer
	out.WriteString(`{"v":1,"op":"call","id":`)
	writeString(&out, call.ID)
	out.WriteString(`,"key":`)
	writeString(&out, call.Key)
	if call.Transport != nil {
		out.WriteString(`,"transport":`)
		writeString(&out, string(*call.Transport))
	}
	writeOptionalJSON(&out, "path", call.Path)
	writeOptionalJSON(&out, "query", call.Query)
	writeOptionalJSON(&out, "headers", call.Headers)
	writeOptionalJSON(&out, "body", call.Body)
	writeOptionalString(&out, "traceId", call.TraceID)
	writeOptionalString(&out, "spanId", call.SpanID)
	out.WriteByte('}')
	return bounded(out.Bytes())
}
func (receipt Receipt) Encode() ([]byte, error) {
	if err := receipt.Validate(); err != nil {
		return nil, err
	}
	var out bytes.Buffer
	out.WriteString(`{"v":1,"op":"receipt","id":`)
	writeString(&out, receipt.ID)
	out.WriteString(`,"key":`)
	writeString(&out, receipt.Key)
	if receipt.Transport != nil {
		out.WriteString(`,"transport":`)
		writeString(&out, string(*receipt.Transport))
	}
	out.WriteString(`,"ok":`)
	out.WriteString(strconv.FormatBool(receipt.OK))
	if receipt.Status != nil {
		out.WriteString(`,"status":`)
		out.WriteString(strconv.Itoa(int(*receipt.Status)))
	}
	writeOptionalJSON(&out, "body", receipt.Body)
	writeOptionalJSON(&out, "error", receipt.Error)
	writeOptionalString(&out, "traceId", receipt.TraceID)
	writeOptionalString(&out, "spanId", receipt.SpanID)
	out.WriteByte('}')
	return bounded(out.Bytes())
}
func bounded(value []byte) ([]byte, error) {
	if len(value) > MaxFrameBytes {
		return nil, fmt.Errorf("frame is %d bytes, over the %d limit", len(value), MaxFrameBytes)
	}
	return value, nil
}
func writeString(out *bytes.Buffer, value string) {
	encoded, _ := json.Marshal(value)
	out.Write(encoded)
}
func writeOptionalString(out *bytes.Buffer, name string, value *string) {
	if value != nil {
		out.WriteString(`,"` + name + `":`)
		writeString(out, *value)
	}
}
func writeOptionalJSON(out *bytes.Buffer, name string, value OptionalJSON) {
	if !value.Present {
		return
	}
	out.WriteString(`,"` + name + `":`)
	var compact bytes.Buffer
	_ = json.Compact(&compact, value.Value)
	out.Write(compact.Bytes())
}
