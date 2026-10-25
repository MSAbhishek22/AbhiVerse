import 'package:abhiverse_controller/core/pairing_code.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('parses the payload shapes the agent can show', () {
    expect(parsePairingCode('123456'), '123456');
    expect(parsePairingCode('  123456  '), '123456');
    expect(parsePairingCode('abhiverse://pair?code=123456'), '123456');
  });

  test('rejects everything else, even text with digits in it', () {
    expect(parsePairingCode(''), isNull);
    expect(parsePairingCode('12345'), isNull);
    expect(parsePairingCode('1234567'), isNull);
    expect(parsePairingCode('https://example.com'), isNull);
    // The old loose fallback accepted these; only the agent's own
    // shapes are trusted now. A poster, screenshot or message with
    // six digits in it must never start a pairing claim to a
    // stranger's computer.
    expect(parsePairingCode('{"code":"654321"}'), isNull);
    expect(parsePairingCode('your code is 246810 — hurry'), isNull);
    expect(parsePairingCode('https://evil.example/pair?code=123456'), isNull);
  });
}
