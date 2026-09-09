// This fixed stdin/stdout test adapter delegates validation to the real client.
package main

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"

	client "github.com/oresoftware/api-docs/clients/go"
)

const limit = 16 * 1024 * 1024

type probeCase struct {
	Name    string  `json:"name"`
	Kind    string  `json:"kind"`
	Encoded *string `json:"encoded"`
}

type request struct {
	Schema string      `json:"schema"`
	Cases  []probeCase `json:"cases"`
}

type result struct {
	Name     string  `json:"name"`
	Kind     string  `json:"kind"`
	Accepted bool    `json:"accepted"`
	Encoded  *string `json:"encoded,omitempty"`
}

func evaluate(row probeCase) (result, error) {
	out := result{Name: row.Name, Kind: row.Kind}
	var encoded []byte
	var err error
	switch row.Kind {
	case "call":
		value, decodeErr := client.DecodeCall([]byte(*row.Encoded))
		if decodeErr != nil {
			return out, nil
		}
		encoded, err = value.Encode()
	case "receipt":
		value, decodeErr := client.DecodeReceipt([]byte(*row.Encoded))
		if decodeErr != nil {
			return out, nil
		}
		encoded, err = value.Encode()
	default:
		return out, errors.New("unknown probe kind")
	}
	// Encode failures after successful decoding are execution failures, not rejects.
	if err != nil {
		return out, err
	}
	text := string(encoded)
	out.Accepted, out.Encoded = true, &text
	return out, nil
}

func run() error {
	if len(os.Args) != 1 {
		return errors.New("probe accepts no arguments")
	}
	input, err := io.ReadAll(io.LimitReader(os.Stdin, limit+1))
	if err != nil {
		return err
	}
	if len(input) > limit {
		return errors.New("probe input exceeds limit")
	}
	decoder := json.NewDecoder(bytes.NewReader(input))
	decoder.DisallowUnknownFields()
	var document request
	if err := decoder.Decode(&document); err != nil {
		return err
	}
	var extra any
	if decoder.Decode(&extra) != io.EOF {
		return errors.New("trailing probe input")
	}
	if document.Schema != "ores.api-docs.rpc-probe/v1" || len(document.Cases) == 0 || len(document.Cases) > 4096 {
		return errors.New("invalid probe schema or coverage")
	}
	names := make(map[string]bool)
	results := make([]result, 0, len(document.Cases))
	for _, row := range document.Cases {
		if row.Name == "" || names[row.Name] || row.Encoded == nil {
			return errors.New("invalid probe name or encoding")
		}
		names[row.Name] = true
		out, err := evaluate(row)
		if err != nil {
			return err
		}
		results = append(results, out)
	}
	return json.NewEncoder(os.Stdout).Encode(struct {
		Schema  string   `json:"schema"`
		Runtime string   `json:"runtime"`
		Results []result `json:"results"`
	}{"ores.api-docs.rpc-probe-result/v1", "go", results})
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "probe execution failed:", err)
		os.Exit(3)
	}
}
