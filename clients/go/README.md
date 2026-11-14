# Go client — RPC v1

The `oresapidocs` Go package handles the **v1** `rpc-call` / `rpc-receipt`
JSON envelope. It is standard-library-only. It is **not** the v2 RIDL frame
protocol under `runtime/go`.

## Bounded TCP streams

TCP uses a four-byte **big-endian** payload length followed by one JSON frame.
To write and read validated v1 envelopes on a socket (or any `io.Writer` /
`io.Reader`):

```go
call := oresapidocs.NewCall("request-1", "healthz")
if err := oresapidocs.WriteLengthPrefixed(conn, call); err != nil {
    return err
}
receipt, err := oresapidocs.ReadReceiptLengthPrefixed(conn)
if err != nil {
    return err
}
return oresapidocs.AssertReceiptForCall(call, receipt)
```

`WriteLengthPrefixed` checks the envelope before writing and completes short
writes; a writer making no progress returns `io.ErrShortWrite`.
`ReadLengthPrefixed` reads **exactly one** frame and checks the declared size
before allocating any payload memory. `ReadCallLengthPrefixed` and
`ReadReceiptLengthPrefixed` also validate the decoded envelope.

Empty frames and declared payloads above `MaxFrameBytes` (8 MiB) are rejected.
Incomplete prefixes and payloads return errors wrapping `io.ErrUnexpectedEOF`.
A clean `io.EOF` when reading the next prefix means the stream ended between
frames. These helpers do not open sockets, set deadlines, or retry after a
partially written frame; callers own connection lifetimes and timeouts.

For already buffered TCP data, `SplitLengthPrefixed` remains available and
returns complete payloads plus a copied trailing incomplete frame.

Run `go test ./...` (or `go test -race ./...`) from `clients/go`.
