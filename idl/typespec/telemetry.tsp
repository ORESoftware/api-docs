// Attribute bag copied into ores-otel fields. No crate edge either way.
// JSON Schema witness: json-schema/telemetry-attributes.schema.json.

namespace Ores.Rpc.Telemetry;

model TelemetryAttributes {
  `rpc.system`: "ores-api-docs";

  @minLength(1)
  `rpc.service`: string;

  @minLength(1)
  @pattern("^[A-Za-z][A-Za-z0-9_]*$")
  `rpc.method`: string;

  `rpc.transport`: Ores.Rpc.V1.Transport;
  `rpc.ok`?: boolean;

  @minValue(100)
  @maxValue(599)
  `http.status_code`?: int32;
}
