/// Extract a 6-digit pairing code from a QR code (or pasted text).
///
/// Accepted payloads — exactly the two shapes the agent produces:
/// - `123456` — the bare code
/// - `abhiverse://pair?code=123456` — the URI the agent encodes
///
/// Anything else is rejected on purpose. The old fallback ("any
/// text containing a 6-digit group") meant any poster, screenshot
/// or message with six digits in it could start a pairing claim
/// to a stranger's computer — and a claim the victim's phone sends
/// is one terminal approval (on the attacker's machine) away from
/// the victim's files going to the wrong laptop. Pairing codes
/// only ever come from your own computer's terminal, so only its
/// two shapes are trusted.
String? parsePairingCode(String raw) {
  final text = raw.trim();
  if (RegExp(r'^\d{6}$').hasMatch(text)) return text;
  final uri = Uri.tryParse(text);
  if (uri != null && uri.scheme == 'abhiverse') {
    final code = uri.queryParameters['code'];
    if (code != null && RegExp(r'^\d{6}$').hasMatch(code)) return code;
  }
  return null;
}
