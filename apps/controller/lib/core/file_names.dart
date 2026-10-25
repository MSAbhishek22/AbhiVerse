// File-name hygiene for transfers, client side.
//
// The server sanitizes names on transfer create too, but the phone
// must never trust a name it did not choose: a compromised or
// foreign server could otherwise hand the phone a name like
// `../../shared_prefs/…` and get it written outside the download
// folder. Basename-only, always.

/// The server's default transfer size cap (ABHIVERSE_MAX_TRANSFER_MB).
/// The client refuses larger files up front instead of uploading
/// for minutes and then failing at completion.
const int maxTransferBytes = 1024 * 1024 * 1024;

/// Returns an error message when [bytes] exceeds the transfer cap,
/// or null when the size is fine.
String? checkTransferSize(int bytes) {
  if (bytes > maxTransferBytes) {
    return 'That file is larger than the 1 GB server limit.';
  }
  return null;
}

/// Reduces any sender-supplied name to a single safe file name.
/// Returns 'download' when nothing usable remains.
String sanitizeDownloadName(String raw) {
  final parts = raw
      .split(RegExp(r'[\\/]'))
      .where((p) => p.isNotEmpty && p != '.' && p != '..');
  if (parts.isEmpty) return 'download';
  final name = parts.last;
  if (name == '.' || name == '..') return 'download';
  return name;
}
