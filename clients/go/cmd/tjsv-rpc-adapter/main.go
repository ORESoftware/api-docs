// This fixed CI adapter has no options. It exercises the public client API;
// it must not learn the expected verdicts or duplicate the schema validator.
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"runtime"

	rpc "github.com/oresoftware/api-docs/clients/go"
)

const maxRequestBytes = 32 * 1024 * 1024

type fixture struct {
	Name    string `json:"name"`
	Kind    string `json:"kind"`
	Encoded string `json:"encoded"`
}

type result struct {
	Name     string  `json:"name"`
	Kind     string  `json:"kind"`
	Accepted bool    `json:"accepted"`
	Encoded  *string `json:"encoded,omitempty"`
}

func run(input io.Reader, output io.Writer) error {
	data, err := io.ReadAll(io.LimitReader(input, maxRequestBytes+1))
	if err != nil {
		return err
	}
	if len(data) > maxRequestBytes {
		return errors.New("adapter request exceeds limit")
	}
	var request struct {
		Cases []fixture `json:"cases"`
	}
	decoder := json.NewDecoder(bytes.NewReader(data))
	decoder.DisallowUnknownFields()
	if err := decoder.Decode(&request); err != nil {
		return fmt.Errorf("invalid adapter request: %w", err)
	}
	if err := decoder.Decode(new(any)); err != io.EOF {
		return errors.New("adapter needs exactly one request")
	}
	if len(request.Cases) == 0 {
		return errors.New("adapter request has no cases")
	}
	results := make([]result, 0, len(request.Cases))
	seen := make(map[string]bool)
	for _, row := range request.Cases {
		if row.Name == "" || seen[row.Name] {
			return errors.New("missing or duplicate case name")
		}
		seen[row.Name] = true
		if row.Encoded == "" || len(row.Encoded) > rpc.MaxFrameBytes {
			return errors.New("case frame size outside profile")
		}
		item := result{Name: row.Name, Kind: row.Kind}
		var encoded []byte
		switch row.Kind {
		case "call":
			value, decodeErr := rpc.DecodeCall([]byte(row.Encoded))
			if decodeErr == nil {
				item.Accepted = true
				encoded, err = value.Encode()
			}
		case "receipt":
			value, decodeErr := rpc.DecodeReceipt([]byte(row.Encoded))
			if decodeErr == nil {
				item.Accepted = true
				encoded, err = value.Encode()
			}
		default:
			return fmt.Errorf("unknown case kind %q", row.Kind)
		}
		// Encoder failures are infrastructure/runtime failures, never negative evidence.
		if err != nil {
			return fmt.Errorf("re-encode %s: %w", row.Name, err)
		}
		if item.Accepted {
			text := string(encoded)
			item.Encoded = &text
		}
		results = append(results, item)
	}
	return json.NewEncoder(output).Encode(struct {
		Schema    string   `json:"schema"`
		Runtime   string   `json:"runtime"`
		Toolchain string   `json:"toolchain"`
		Results   []result `json:"results"`
	}{"ores.api-docs.rpc-adapter/v1", "go", runtime.Version(), results})
}

func main() {
	if len(os.Args) != 1 {
		fmt.Fprintln(os.Stderr, "this fixed adapter accepts no arguments")
		os.Exit(3)
	}
	if err := run(os.Stdin, os.Stdout); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(3)
	}
}
