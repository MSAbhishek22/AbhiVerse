import '../settings/doc_screen.dart';

/// The in-app legal + help texts: Terms, Privacy & Security, FAQ.
/// Written the way the app talks — blunt, casual, zero legalese fog —
/// because a wall of lawyer-speak is how users miss the parts that
/// actually matter. The liability section does not hide: if you
/// misconfigure your own setup, that is on you, and it says so.
///
/// Keep these in step with the website FAQ and SECURITY.md when the
/// security model changes; a stale promise here is worse than none.

const termsSections = [
  DocSection(
    'The short version',
    'Abhiverse is free, open-source software (MIT) made with love by '
    'M S Abhishek. No account, no subscription, no ads, no tracking. '
    'Use it, fork it, self-host it, break it and fix it — that is the deal.',
  ),
  DocSection(
    'What you get',
    'Your phone becomes the remote for your laptop: see your devices, '
    'send files with SHA-256 verification, and view + control your '
    'screen over WebRTC. Remote input only works after you approve the '
    'device AND the laptop agent is started with input explicitly allowed.',
  ),
  DocSection(
    'The blunt part — your responsibility',
    'Read this one properly, because it is the whole point.\n\n'
    '• You approve your own devices. If you approve a device that is '
    'not yours, whatever it does afterwards is your fault.\n\n'
    '• You run or pick your own server. If you expose it to the whole '
    'internet without HTTPS, ignore the cleartext warning, or leave '
    'your server address lying around — any leak that follows is your '
    'fault, not the app\'s.\n\n'
    '• You revoke lost devices. Lose your phone and do nothing about '
    'it? Whoever holds it holds your remote. Revoke it from the '
    'device list the second you notice.\n\n'
    '• LAN mode allows cleartext http on purpose, for your home Wi-Fi. '
    'Do not use it on public Wi-Fi, cafés, or airports. Seriously.',
  ),
  DocSection(
    'What we never do',
    'No telemetry, no analytics, no ads SDKs, no account server. Your '
    'device keys never leave your devices. We cannot see your files, '
    'your screen, or your keystrokes — and we do not want to.',
  ),
  DocSection(
    'No warranty, no promises',
    'This is beta software, provided as-is, with no warranty of any '
    'kind. Transfers can fail halfway, sessions can drop, a bug can '
    'eat your afternoon. Back up anything you cannot afford to lose. '
    'The app is free, so our liability stops at the app itself — there '
    'is no paid tier to refund and no damages to claim.',
  ),
  DocSection(
    'Support, if you feel like it',
    'Abhiverse stays free forever. If it saves you time, you can '
    'support the dev via UPI in Settings → Support. Zero pressure, '
    'zero paywall either way. Made with love, kept alive by goodwill.',
  ),
  DocSection(
    'The boring legal bit',
    'Copyright © 2026 ABHIVERSE contributors, MIT licensed. If anything '
    'here ever needs a courtroom, the laws of India apply. The app is '
    'a tool; what you point it at and who you approve is your call.',
  ),
];

const securitySections = [
  DocSection(
    'Keys stay on your devices',
    'Every device makes its own Ed25519 keypair on first run and keeps '
    'it in secure storage. Keys are never uploaded, emailed, or synced '
    'anywhere. Lose the device, lose the key — that is the trade-off '
    'that makes the whole thing work.',
  ),
  DocSection(
    'Pairing is a handshake, not a password',
    'Pairing uses a single-use 6-digit code that expires in 5 minutes, '
    'and the target computer must explicitly approve it on its own '
    'screen. No approval, no pairing. Guessing is throttled, and '
    'pending pairings are capped and pruned.',
  ),
  DocSection(
    'Every request is signed',
    'After pairing, every request is signed with the device key and '
    'checked for replays inside a 120-second window. Revocation is '
    'instant: a revoked device loses reads, transfers, and live '
    'sessions immediately.',
  ),
  DocSection(
    'Transfers, honestly',
    'Files move in 256 KiB chunks with SHA-256 verification and a '
    '1 GiB cap. Chunks relay briefly through the coordination server '
    'you point the app at and are pruned after 7 days. Self-host the '
    'server and even that relay stays on your own network.',
  ),
  DocSection(
    'What we collect: nothing',
    'No accounts, no analytics, no crash reporting, no ads. The app '
    'asks for exactly two permissions: CAMERA (to scan the pairing QR) '
    'and INTERNET (to talk to your server). That is the whole list.',
  ),
  DocSection(
    'Honest limits — read this',
    '• LAN http is cleartext by design. Fine at home, dangerous in '
    'public. Hosted servers must use https.\n'
    '• The coordination server sees device names, presence, and '
    'transfer chunks in transit. Self-host if that bothers you.\n'
    '• An approved device is trusted until you revoke it. Approval is '
    'the security boundary — treat it like handing over your keyboard.\n'
    '• This is beta: real-device testing is still in progress, so '
    'WebRTC media, screen capture, and input injection are not yet '
    'proven on every machine.',
  ),
  DocSection(
    'If something feels wrong',
    'Revoke the device from the device list, switch your server URL, '
    'and report it on GitHub (MSAbhishek22/AbhiVerse). A fast revoke '
    'beats a perfect post-mortem.',
  ),
];

const faqSections = [
  DocSection(
    'What even is Abhiverse?',
    'Your phone is the remote for your laptop. See your devices, send '
    'files, and view + control your screen — without an account, a '
    'subscription, or a cloud company in the middle of your life.',
  ),
  DocSection(
    'Do I need an account?',
    'No. No signup, no password, no email, no phone number. Your '
    'devices recognise each other by keys, not by who you logged in as.',
  ),
  DocSection(
    'Where does my stuff go?',
    'Signalling and file chunks go through the coordination server you '
    'point the app at — the default hosted one, or your own if you '
    'self-host. Keys never leave your devices, and nothing is sold, '
    'mined, or analysed, because there is nobody collecting it.',
  ),
  DocSection(
    'Is it actually free?',
    'Yes. MIT open source, free forever. Support via UPI in Settings '
    'is optional and changes nothing about the app.',
  ),
  DocSection(
    'What do I need to run it?',
    'An Android phone, a Windows laptop, and the Abhiverse agent '
    'running on the laptop. Same Wi-Fi works great; the internet works '
    'too, through the coordination server and TURN relay.',
  ),
  DocSection(
    'Is it safe?',
    'Short answer: as safe as your approvals. Keys, signing, replay '
    'protection, and revocation are all real — see Privacy & Security '
    'for the full story, including the honest limits.',
  ),
  DocSection(
    'What if I approve the wrong device?',
    'Revoke it immediately from the device list. Revocation kills its '
    'reads, transfers, and live sessions on the spot. Then figure out '
    'how it got your code and do not do that again.',
  ),
  DocSection(
    'Why is it beta?',
    'Because the final exam is your hardware. WebRTC media, screen '
    'capture, and input injection work in the lab; proving them on '
    'real laptops and phones is the last step before a real release.',
  ),
  DocSection(
    'Can I self-host everything?',
    'Yes — the server is in the repo (server/api plus a deploy bundle '
    'with Caddy and coturn). Point the app at your URL in Settings → '
    'Server and the hosted service never sees a byte.',
  ),
  DocSection(
    'How do I support the project?',
    'Settings → Support has the UPI ID. Star the repo, report bugs, '
    'or just tell someone. Made with love, kept alive by goodwill.',
  ),
];
