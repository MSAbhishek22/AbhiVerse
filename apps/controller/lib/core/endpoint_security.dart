/// Guards for the API base URL the phone connects to.
///
/// Why this exists: the phone's API URL used to be build-time only
/// and was trusted blindly. An `http` URL to a *public* server
/// would send pairing codes, signatures, file bytes and SDP
/// (network details) in the clear — and a mistyped or pasted URL
/// is exactly how that happens. So:
/// - `https` anywhere: allowed.
/// - `http` to loopback, a private LAN range, or a `.local` host:
///   allowed, with a warning (LAN use is the documented dev setup).
/// - `http` to a public host: blocked outright, with a message
///   that says what to do instead.
///
/// This is a client-side guard, not a replacement for the server's
/// signature checks: even a hostile server cannot forge a paired
/// device's signatures, but it could read cleartext traffic, so
/// cleartext to public hosts is refused before any request goes out.
class EndpointCheck {
  const EndpointCheck({required this.blocked, this.warning, this.message});

  /// True when the URL must not be used at all.
  final bool blocked;

  /// Non-blocking caution shown while connected (LAN http).
  final String? warning;

  /// User-facing explanation when [blocked] is true.
  final String? message;
}

EndpointCheck checkApiBaseUrl(String raw) {
  final uri = Uri.tryParse(raw.trim());
  if (uri == null ||
      uri.host.isEmpty ||
      (uri.scheme != 'http' && uri.scheme != 'https')) {
    return const EndpointCheck(
      blocked: true,
      message:
          'That server address doesn\'t look right — it should look '
          'like http://192.168.1.20:8080 or an https URL.',
    );
  }
  if (uri.scheme == 'https') {
    return const EndpointCheck(blocked: false);
  }
  final host = uri.host.toLowerCase();
  if (host == 'localhost' ||
      host.endsWith('.localhost') ||
      host.endsWith('.local') ||
      host == '::1' ||
      host == '[::1]' ||
      _isLoopbackOrPrivateIp(host)) {
    return const EndpointCheck(
      blocked: false,
      warning:
          'Local-network connection (http). For a hosted setup, '
          'use https.',
    );
  }
  return const EndpointCheck(
    blocked: true,
    message:
        'Refusing an unencrypted (http) connection to a public '
        'server — pairing codes and files would travel in the clear. '
        'Use https, or a local-network address (192.168.x.x, 10.x.x.x, '
        'localhost).',
  );
}

bool _isLoopbackOrPrivateIp(String host) {
  final parts = host.split('.');
  if (parts.length != 4) {
    // IPv6 link-local (fe80::/10) is LAN-only by construction.
    return host.startsWith('fe80:');
  }
  final octets = <int>[];
  for (final part in parts) {
    final value = int.tryParse(part);
    if (value == null || value < 0 || value > 255) return false;
    octets.add(value);
  }
  // 127.0.0.0/8 loopback.
  if (octets[0] == 127) return true;
  // 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16.
  if (octets[0] == 10) return true;
  if (octets[0] == 172 && octets[1] >= 16 && octets[1] <= 31) return true;
  if (octets[0] == 192 && octets[1] == 168) return true;
  if (octets[0] == 169 && octets[1] == 254) return true;
  return false;
}
