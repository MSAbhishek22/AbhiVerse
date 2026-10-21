# ABHIVERSE — Protocol (v1, draft)

## GET /health

Response `200 OK`, `application/json`:

```json
{
  "app": "ABHIVERSE",
  "service": "abhiverse-api",
  "status": "ok",
  "version": "0.2.0",
  "protocol_version": 1
}
```

Defined in `crates/protocol` as `HealthResponse` (Rust) and mirrored
in the controller at `apps/controller/lib/core/health_client.dart`.


## Device identity (Task 002)

`DeviceInfo`: `device_id` is the first 16 bytes of SHA-256 of the
Ed25519 public key (hex). The API rejects registrations whose
`device_id` does not match the key fingerprint.

## Pairing (Task 003)

- `POST /pairing/requests` `{agent: DeviceInfo}` → `201 {pairing_id, code, expires_at}`.
  Codes are 6 digits, live 5 minutes, single-use.
- `POST /pairing/claim` `{code, controller: DeviceInfo}` → `200 PairingView`
  (`404` unknown/expired code, `409` code already used, `400` if the
  controller's `device_id` is not the fingerprint of its public key).
  Since v0.5 the controller registers its Ed25519 identity here; the
  API stores the key and verifies this controller's signatures from
  then on. The agent also prints the code as a QR
  (`abhiverse://pair?code=…`) for the app's scanner.
- `GET /pairing/{pairing_id}` → `PairingView`.
- `POST /pairing/{pairing_id}/approve` `{signature}` — hex Ed25519
  signature by the agent over `abhiverse-pairing-approve:{pairing_id}:{code}`.
  Only valid from `claimed` state (`401` bad signature, `409` wrong state, `410` expired).
- `POST /pairing/{pairing_id}/reject` — same, over
  `abhiverse-pairing-reject:{pairing_id}:{code}`.

## Presence & revocation (Task 004)

- `GET /devices` → `{devices: [DeviceView]}` for paired, unrevoked
  devices. `online` = heartbeat within the last 30s.
- `POST /devices/{device_id}/heartbeat` `{timestamp, signature}` —
  signature over `abhiverse-heartbeat:{device_id}:{timestamp}`;
  timestamps must be within 120s of server time (replay guard).
  Revoked devices get `403`.
- `DELETE /devices/{device_id}` `{timestamp, signature}` — self-
  revocation; signature over `abhiverse-revoke:{device_id}:{timestamp}`.

API state is in-memory in v0.2 (restart clears pairings); persistent
storage is planned with the hosted-service phase.


## File transfer (Task 005)

Files move in 256 KiB chunks through the API relay (peer-to-peer
arrives with WebRTC). The sender declares the whole file's SHA-256
up front; the server verifies it when the transfer completes, and
receivers verify again locally after assembly.

- `POST /transfers` `CreateTransferRequest` → `201 TransferView`.
  Agent senders must sign `abhiverse-transfer-create:{from}:{to}:{sha256}:{ts}`
  (headers `x-abhiverse-timestamp`, `x-abhiverse-signature`).
  Controller senders must be the target device's paired controller.
- `PUT /transfers/{id}/chunks/{index}` — raw chunk bytes. Agent
  senders sign `abhiverse-chunk:{id}:{index}:{chunk_sha256}:{ts}`.
  Re-uploading a chunk is idempotent, so uploads resume by asking
  `GET /transfers/{id}` which indexes are still missing.
- `GET /transfers/{id}` → `TransferView` (with `received_chunks`).
- `GET /transfers?for={recipient_id}` → transfers for a recipient.
- `GET /transfers/{id}/chunks/{index}` — agent recipients sign
  `abhiverse-download:{id}:{index}:{ts}`; controller recipients use
  the unguessable transfer ID as the v0.3 capability.
- `POST /transfers/{id}/complete` — server assembles and compares
  the SHA-256: match → `complete`; mismatch → `422`, status `failed`.

Chunks live under `ABHIVERSE_TRANSFER_DIR` (default: a per-process
temp directory) as one `.part` file per transfer.


## Remote desktop signalling (Task 006a)

Full design in `docs/remote-desktop.md`. Summary:

- `GET /config/rtc` → `RtcConfig` (STUN default; TURN via
  `ABHIVERSE_ICE_SERVERS_JSON`).
- `POST /sessions {device_id, controller_id, sdp_offer}` → `201`.
  Since v0.5 the controller must sign the request
  (`abhiverse-session-create:{device_id}:{controller_id}:{sha256(offer)}:{ts}`),
  and transfer creates (`abhiverse-transfer-create:…`) and
  revocations (`abhiverse-revoke:…`) are likewise accepted from
  either the device's key or its paired controller's key.
  (paired controller only; offers expire after 120 s).
- `GET /sessions/{id}?as={participant}` / `GET /sessions?for={device_id}`.
- `POST /sessions/{id}/answer {sdp_answer}` — agent-signed over
  `abhiverse-session-answer:{id}:{sha256(answer)}:{ts}`; `/reject`
  and agent `/end` are signed the same way.
- `POST|GET /sessions/{id}/ice` — trickle ICE with sequence numbers.
- `POST /sessions/{id}/end {from_id}`.
- Input events travel on the WebRTC data channel `abhiverse-input`
  as tagged JSON (`InputEvent`), never through the API.
