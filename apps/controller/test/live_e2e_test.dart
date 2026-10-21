// Live end-to-end test against a real coordination API.
// Runs ONLY when ABHIVERSE_E2E=1 (with ABHIVERSE_E2E_CODE = the
// pairing code shown by the agent). Skipped (vacuous) otherwise.
import 'dart:convert';
import 'dart:io';

import 'package:abhiverse_controller/core/api_client.dart';
import 'package:abhiverse_controller/core/config.dart';
import 'package:abhiverse_controller/core/controller_identity.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final enabled = Platform.environment['ABHIVERSE_E2E'] == '1';
  if (!enabled) {
    test('live e2e disabled', () {}, skip: 'set ABHIVERSE_E2E=1');
    return;
  }
  final apiUrl =
      Platform.environment['ABHIVERSE_E2E_API'] ?? 'http://127.0.0.1:8080';
  final code = Platform.environment['ABHIVERSE_E2E_CODE']!;
  final phase = Platform.environment['ABHIVERSE_E2E_PHASE'] ?? 'main';

  testWidgets('live: claim, sign, transfer, session, revoke', (tester) async {
    // The widget-test binding replaces HttpClient with a mock that
    // blocks all real networking; this test is explicitly a live
    // test, so restore the real client.
    HttpOverrides.global = null;
    await tester.runAsync(() async {
      // Same fixed seed in both phases -> same controller device.
      final identity = await ControllerIdentity.fromSeed(
        List.filled(32, 21),
        name: 'E2E Phone',
      );
      final api = ApiClient(
        config: AbhiverseEndpointConfig(apiBaseUrl: apiUrl),
        identity: identity,
      );
      if (phase == 'main') {
        final claim = await api.claimPairing(
          code: code,
          controllerName: 'E2E Phone',
        );
        expect(claim.status, 'claimed');
        // Wait for the human-at-the-agent approval (the driver sends "y").
        DeviceView? device;
        for (var i = 0; i < 40 && device == null; i++) {
          final devices = await api.fetchDevices();
          if (devices.isNotEmpty) device = devices.first;
          if (device == null) {
            await Future<void>.delayed(const Duration(milliseconds: 500));
          }
        }
        expect(device, isNotNull, reason: 'device appears after approval');
        File('/tmp/e2e_device_id.txt').writeAsStringSync(device!.deviceId);

        // Signed controller->agent transfer.
        final bytes = utf8.encode('signed hello from the Dart controller');
        final view = await api.sendBytes(
          toId: device.deviceId,
          fileName: 'e2e-note.txt',
          bytes: bytes,
        );
        expect(view.status, 'complete');
        File('/tmp/e2e_transfer_id.txt').writeAsStringSync(view.transferId);

        // Signed session create (offer content is opaque to the API).
        final session = await api.createSession(
          deviceId: device.deviceId,
          sdpOffer: 'v=0 e2e',
        );
        expect(session.status, 'offered');
        await api.endSession(session.sessionId);
      } else if (phase == 'after-restart') {
        // API was restarted with the same state file: pairing survived.
        final devices = await api.fetchDevices();
        expect(devices, hasLength(1));
        final deviceId = devices.first.deviceId;
        // Controller-signed revocation ends the pairing.
        await api.revokeDevice(deviceId);
        final after = await api.fetchDevices();
        expect(after, isEmpty);
      }
    });
  }, timeout: const Timeout(Duration(seconds: 60)));
}
