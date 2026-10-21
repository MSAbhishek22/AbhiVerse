import 'dart:convert';

import 'package:abhiverse_controller/core/api_client.dart';
import 'package:abhiverse_controller/core/config.dart';
import 'package:abhiverse_controller/core/health_client.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';

void main() {
  const config = AbhiverseEndpointConfig(apiBaseUrl: 'http://localhost:8080');

  test('fetchDevices parses the device list', () async {
    final mock = MockClient((request) async {
      expect(request.url.path, '/devices');
      return http.Response(
        jsonEncode({
          'devices': [
            {
              'info': {
                'device_id': 'abc123',
                'name': 'My Laptop',
                'platform': 'windows',
                'kind': 'agent',
                'public_key': 'ff',
              },
              'paired_with_name': 'My Phone',
              'last_seen': 1700000000,
              'online': true,
            }
          ],
        }),
        200,
      );
    });
    final client = ApiClient(config: config, client: mock);
    final devices = await client.fetchDevices();
    expect(devices, hasLength(1));
    expect(devices.single.name, 'My Laptop');
    expect(devices.single.online, isTrue);
  });

  test('claimPairing posts the code and parses the agent name', () async {
    final mock = MockClient((request) async {
      final body = jsonDecode(request.body) as Map<String, dynamic>;
      expect(body['code'], '123456');
      expect(body['controller_name'], 'Pixel');
      return http.Response(
        jsonEncode({
          'pairing_id': 'p1',
          'status': 'claimed',
          'agent': {
            'device_id': 'abc123',
            'name': 'My Laptop',
            'platform': 'windows',
            'kind': 'agent',
            'public_key': 'ff',
          },
          'controller_id': 'c1',
          'controller_name': 'Pixel',
          'expires_at': 1700000300,
        }),
        200,
      );
    });
    final client = ApiClient(config: config, client: mock);
    final claim = await client.claimPairing(code: '123456', controllerName: 'Pixel');
    expect(claim.agentName, 'My Laptop');
    expect(claim.status, 'claimed');
  });

  test('claimPairing maps 404 to a friendly exception', () async {
    final mock = MockClient((_) async => http.Response('{"error":"x"}', 404));
    final client = ApiClient(config: config, client: mock);
    expect(
      client.claimPairing(code: '000000', controllerName: 'Pixel'),
      throwsA(isA<AbhiverseApiException>()),
    );
  });
}
