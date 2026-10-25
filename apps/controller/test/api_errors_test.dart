import 'dart:convert';

import 'package:abhiverse_controller/core/api_client.dart';
import 'package:abhiverse_controller/core/config.dart';
import 'package:abhiverse_controller/core/health_client.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';

ApiClient clientWith(http.Client mock) => ApiClient(
  config: const AbhiverseEndpointConfig(apiBaseUrl: 'http://localhost:8080'),
  client: mock,
  controllerId: 'test-controller',
);

void main() {
  test('server error text surfaces instead of a bare status', () async {
    const hint =
        'signature timestamp outside allowed window - check '
        'that both devices\' clocks are in sync (within 2 minutes)';
    final mock = MockClient((request) async {
      if (request.url.path == '/devices') {
        return http.Response(jsonEncode({'error': hint}), 401);
      }
      return http.Response('not found', 404);
    });
    await expectLater(
      clientWith(mock).fetchDevices(),
      throwsA(
        isA<AbhiverseApiException>().having(
          (e) => e.message,
          'message',
          contains('clocks are in sync'),
        ),
      ),
    );
  });

  test('non-JSON error bodies fall back to the status message', () async {
    final mock = MockClient(
      (_) async => http.Response('<html>boom</html>', 500),
    );
    await expectLater(
      clientWith(mock).fetchDevices(),
      throwsA(
        isA<AbhiverseApiException>().having(
          (e) => e.message,
          'message',
          contains('HTTP 500'),
        ),
      ),
    );
  });
}
