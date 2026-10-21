/// Extract a 6-digit pairing code from whatever a QR code (or a
/// pasted string) contains.
///
/// Accepted payloads:
/// - `123456` — the bare code
/// - `abhiverse://pair?code=123456` — the URI the agent encodes
/// - any text containing `code=123456` or a lone 6-digit group
String? parsePairingCode(String raw) {
  final text = raw.trim();
  if (RegExp(r'^\d{6}$').hasMatch(text)) return text;
  final uri = Uri.tryParse(text);
  final fromQuery = uri?.queryParameters['code'];
  if (fromQuery != null && RegExp(r'^\d{6}$').hasMatch(fromQuery)) {
    return fromQuery;
  }
  final match = RegExp(r'code=(\d{6})').firstMatch(text) ??
      RegExp(r'(?<!\d)(\d{6})(?!\d)').firstMatch(text);
  return match?.group(1);
}
