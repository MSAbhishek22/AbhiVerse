# ABHIVERSE — Protocol (v1, draft)

## GET /health

Response `200 OK`, `application/json`:

```json
{
  "app": "ABHIVERSE",
  "service": "abhiverse-api",
  "status": "ok",
  "version": "0.1.0",
  "protocol_version": 1
}
```

Defined in `crates/protocol` as `HealthResponse` (Rust) and mirrored
in the controller at `apps/controller/lib/core/health_client.dart`.
