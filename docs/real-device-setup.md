# Real-device setup (Windows laptop + Android phone)

How to build and run Abhiverse v0.5.2 on real hardware for the
validation run in `docs/real-device-test.md`. Everything runs as
your normal user — never as administrator.

## 1. Build (on the Windows laptop)

Prerequisites: Rust (rustup, stable), Flutter SDK (stable) with
Android SDK + a phone in USB-debugging mode (or an APK sideload).

```powershell
git clone https://github.com/MSAbhishek22/AbhiVerse.git
cd AbhiVerse
cargo build --release -p abhiverse-api -p abhiverse-agent-windows
# -> target\release\abhiverse-api.exe, abhiverse-agent.exe
cd apps\controller
flutter build apk --release --dart-define=ABHIVERSE_API_BASE_URL=http://<laptop-LAN-IP>:8080
# -> build\app\outputs\flutter-apk\app-release.apk
```

Install the APK on the phone (file transfer + "Install via USB",
or `adb install app-release.apk`).

## 2. Run the API

The API binds to localhost by default. For a phone on the same
Wi-Fi it must listen on the LAN interface — do this only on a
trusted network:

```powershell
$env:ABHIVERSE_BIND_ADDR = "<laptop-LAN-IP>:8080"
$env:ABHIVERSE_STATE_FILE = "$PWD\abhiverse-state.json"
.\target\release\abhiverse-api.exe
```

Check from the phone's browser: `http://<laptop-LAN-IP>:8080/health`
should return `"status":"ok"`. Windows Firewall may prompt the
first time — allow on Private networks only.

USB alternative (no LAN exposure at all): keep the API on
localhost, run `adb reverse tcp:8080 tcp:8080`, and build the app
with `ABHIVERSE_API_BASE_URL=http://127.0.0.1:8080`.

## 3. Pair

```powershell
.\target\release\abhiverse-agent.exe --pair
```

The terminal shows a 6-digit code and a QR code. In the app:
Devices → Pair → scan the QR (or type the code) → watch the
laptop terminal and approve with `y`. Both sides keep their own
Ed25519 identity: the laptop's at `%USERPROFILE%\.abhiverse\agent\keypair.json`
(verify it is not world-readable; the agent sets a current-user
DACL and refuses to start unprotected), the phone's in Android
secure storage.

Verify the key file ACL (optional):

```powershell
icacls "$env:USERPROFILE\.abhiverse\agent\keypair.json"
# expect: only your user (F), inheritance removed
```

## 4. Everyday runs

```powershell
# Presence + file transfer (safe default: no input control)
.\target\release\abhiverse-agent.exe

# Remote desktop answering (view-only unless the flag is added)
.\target\release\abhiverse-agent.exe --remote

# Remote desktop with input — ONLY for the input test
.\target\release\abhiverse-agent.exe --remote --allow-input
```

## 5. Stopping and revoking (know these before the test)

- Stop the agent: Ctrl+C in its terminal — remote input ends with
  the process.
- Revoke from the phone: device tile → ⋮ → Revoke pairing.
- Revoke from the laptop: `abhiverse-agent.exe --revoke`.
- Revocation is immediate and, since v0.5.2, also ends any live
  remote session.
