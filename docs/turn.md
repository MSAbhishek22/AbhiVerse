# TURN deployment guide (Task 006d)

WebRTC connects peers directly whenever it can (STUN). On networks
where both sides sit behind symmetric NATs or strict firewalls,
direct paths fail and media must be relayed by a **TURN** server.
ABHIVERSE ships with a STUN-only default; add TURN when you deploy
for real use across the internet.

## How ABHIVERSE distributes ICE config

Peers never hardcode servers. Both the agent and the controller app
fetch `GET /config/rtc` from the coordination API at session start.
The API returns STUN by default, or whatever JSON is set in the
`ABHIVERSE_ICE_SERVERS_JSON` environment variable:

```bash
export ABHIVERSE_ICE_SERVERS_JSON='[
  {"urls": ["stun:stun.l.google.com:19302"]},
  {"urls": ["turn:turn.example.com:3478?transport=udp",
             "turn:turn.example.com:3478?transport=tcp",
             "turns:turn.example.com:5349"],
   "username": "abhiverse",
   "credential": "REPLACE_WITH_A_STRONG_SECRET"}
]'
```

Restart the API after changing it. Credentials in this JSON are
visible to every paired peer (they need them to use the relay) —
treat them as *scoped relay credentials*, never reuse other
passwords, and rotate them if a device is lost.

## Running coturn with Docker

```yaml
# docker-compose.yml
services:
  coturn:
    image: coturn/coturn:latest
    network_mode: host          # TURN needs many UDP ports; host mode is simplest
    command: >
      --listening-port=3478
      --tls-listening-port=5349
      --realm=turn.example.com
      --server-name=turn.example.com
      --lt-cred-mech
      --user=abhiverse:REPLACE_WITH_A_STRONG_SECRET
      --cert=/certs/fullchain.pem
      --pkey=/certs/privkey.pem
      --min-port=49152
      --max-port=65535
      --no-multicast-peers
    volumes:
      - /etc/letsencrypt/live/turn.example.com:/certs:ro
    restart: unless-stopped
```

## Firewall checklist

| Port        | Proto   | Purpose                  |
|-------------|---------|--------------------------|
| 3478        | UDP+TCP | TURN/STUN listening      |
| 5349        | TCP     | TURNS (TLS)              |
| 49152-65535 | UDP     | Relayed media range      |

Point `turn.example.com` DNS at the host, open the ports above, and
verify from outside with a tool like Trickle ICE
(`https://webrtc.github.io/samples/src/content/peerconnection/trickle-ice/`)
using the same URL/username/credential as your API config: you want
at least one `relay` candidate.

## Notes & limits

- **Long-term credentials** (as above) are the simple option. For a
  public hosted deployment, prefer coturn's `--use-auth-secret`
  with short-lived, per-session credentials minted by the API —
  tracked as future work; the static JSON above is the v1 route.
- TURN relays encrypted DTLS traffic; the relay operator cannot see
  screen content or input, only packet sizes and timing.
- Self-hosters can run coturn on the same box as the API; the API
  itself only serves the JSON config and never touches media.
