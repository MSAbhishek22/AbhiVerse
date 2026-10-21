import 'package:abhiverse_controller/core/controller_identity.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter_test/flutter_test.dart';

String hexOf(List<int> bytes) =>
    bytes.map((b) => b.toRadixString(16).padLeft(2, '0')).join();

void main() {
  test('device id is the sha-256 fingerprint of the public key', () async {
    final identity = await ControllerIdentity.fromSeed(List.filled(32, 42));
    final pubBytes = <int>[];
    for (var i = 0; i < 32; i++) {
      pubBytes.add(
          int.parse(identity.publicKeyHex.substring(i * 2, i * 2 + 2), radix: 16));
    }
    final expected = hexOf(sha256.convert(pubBytes).bytes.sublist(0, 16));
    expect(identity.deviceId, expected);
    expect(identity.deviceId.length, 32);
  });

  test('same seed gives same identity and deterministic signatures', () async {
    final a = await ControllerIdentity.fromSeed(List.filled(32, 1));
    final b = await ControllerIdentity.fromSeed(List.filled(32, 1));
    expect(a.deviceId, b.deviceId);
    final sig = await a.sign('abhiverse-revoke:dev:1');
    expect(sig.length, 128);
    expect(sig, await b.sign('abhiverse-revoke:dev:1'));
    final other = await ControllerIdentity.fromSeed(List.filled(32, 2));
    expect(other.deviceId, isNot(a.deviceId));
  });

  test('signing messages match the Rust protocol formats', () {
    expect(
      transferCreateMessage(fromId: 'a', toId: 'b', sha256Hex: 'cc', timestamp: 7),
      'abhiverse-transfer-create:a:b:cc:7',
    );
    expect(
      sessionCreateMessage(
          deviceId: 'd', controllerId: 'c', offerSha256Hex: 'ee', timestamp: 9),
      'abhiverse-session-create:d:c:ee:9',
    );
    expect(revokeMessage(deviceId: 'd', timestamp: 3), 'abhiverse-revoke:d:3');
  });
}
