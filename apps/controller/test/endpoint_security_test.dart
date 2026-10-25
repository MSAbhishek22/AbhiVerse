import 'package:abhiverse_controller/core/endpoint_security.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('https anywhere is allowed with no warning', () {
    final check = checkApiBaseUrl('https://api.example.com:443');
    expect(check.blocked, isFalse);
    expect(check.warning, isNull);
  });

  test('http on loopback and private LAN ranges is allowed with a warning', () {
    for (final url in [
      'http://localhost:8080',
      'http://127.0.0.1:8080',
      'http://10.0.2.2:8080',
      'http://10.20.30.40:8080',
      'http://172.16.0.9:8080',
      'http://172.31.255.1:8080',
      'http://192.168.1.20:8080',
      'http://169.254.10.20:8080',
      'http://mybox.local:8080',
    ]) {
      final check = checkApiBaseUrl(url);
      expect(check.blocked, isFalse, reason: url);
      expect(check.warning, isNotNull, reason: url);
    }
  });

  test('http on public hosts is blocked', () {
    for (final url in [
      'http://8.8.8.8:8080',
      'http://example.com:8080',
      'http://172.15.0.1:8080',
      'http://172.32.0.1:8080',
      'http://192.169.1.1:8080',
    ]) {
      final check = checkApiBaseUrl(url);
      expect(check.blocked, isTrue, reason: url);
      expect(check.message, contains('https'));
    }
  });

  test('garbage and non-http(s) URLs are blocked', () {
    for (final url in ['', 'not a url', 'ftp://example.com', '://missing']) {
      expect(checkApiBaseUrl(url).blocked, isTrue, reason: url);
    }
  });
}
