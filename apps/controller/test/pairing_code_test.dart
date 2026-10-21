import 'package:abhiverse_controller/core/pairing_code.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('parses the payload shapes the agent can show', () {
    expect(parsePairingCode('123456'), '123456');
    expect(parsePairingCode('  123456  '), '123456');
    expect(parsePairingCode('abhiverse://pair?code=123456'), '123456');
    expect(parsePairingCode('{"code":"654321"}'), '654321');
    expect(parsePairingCode('your code is 246810 — hurry'), '246810');
  });

  test('rejects payloads without a 6-digit code', () {
    expect(parsePairingCode(''), isNull);
    expect(parsePairingCode('12345'), isNull);
    expect(parsePairingCode('1234567'), isNull);
    expect(parsePairingCode('https://example.com'), isNull);
  });
}
