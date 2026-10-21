import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:abhiverse_controller/core/config.dart';
import 'package:abhiverse_controller/core/health_client.dart';

void main() {
  const config = AbhiverseEndpointConfig(apiBaseUrl: 'http://localhost:8080');

  test('parses a valid health response', () async {
    final mock = MockClient((request) async {
      expect(request.url.path, '/health');
      return http.Response(
        jsonEncode({
          'app': 'ABHIVERSE',
          'service': 'abhiverse-api',
          'status': 'ok',
          'version': '0.1.0',
          'protocol_version': 1,
        }),
        200,
      );
    });
    final client = HealthClient(config: config, client: mock);
    final health = await client.fetchHealth();
    expect(health.app, 'ABHIVERSE');
    expect(health.status, 'ok');
    expect(health.version, '0.1.0');
    expect(health.protocolVersion, 1);
  });

  test('non-200 becomes a AbhiverseApiException', () async {
    final mock = MockClient((_) async => http.Response('nope', 500));
    final client = HealthClient(config: config, client: mock);
    expect(client.fetchHealth(), throwsA(isA<AbhiverseApiException>()));
  });

  test('network failure becomes a AbhiverseApiException', () async {
    final mock = MockClient((_) async => throw Exception('offline'));
    final client = HealthClient(config: config, client: mock);
    expect(client.fetchHealth(), throwsA(isA<AbhiverseApiException>()));
  });

  test('malformed JSON becomes a AbhiverseApiException', () async {
    final mock =
        MockClient((_) async => http.Response('{"app": 42}', 200));
    final client = HealthClient(config: config, client: mock);
    expect(client.fetchHealth(), throwsA(isA<AbhiverseApiException>()));
  });
}
