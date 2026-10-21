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
  test('sendBytes hashes, chunks and completes a transfer', () async {
    final paths = <String>[];
    final mock = MockClient((request) async {
      paths.add('${request.method} ${request.url.path}');
      if (request.url.path == '/transfers') {
        final body = jsonDecode(request.body) as Map<String, dynamic>;
        expect(body['file_name'], 'hello.txt');
        expect(body['sha256'],
            '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824'); // sha256("hello")
        expect(body['total_chunks'], 1);
        return http.Response(
          jsonEncode({
            'transfer_id': 't1', 'from_id': body['from_id'], 'to_id': 'dev1',
            'file_name': 'hello.txt', 'size_bytes': 5,
            'sha256': body['sha256'], 'total_chunks': 1,
            'received_chunks': <int>[], 'status': 'receiving',
          }),
          201,
        );
      }
      if (request.url.path == '/transfers/t1/chunks/0') {
        expect(request.bodyBytes, utf8.encode('hello'));
        return http.Response('{"received":1,"total":1}', 200);
      }
      if (request.url.path == '/transfers/t1/complete') {
        return http.Response(
          jsonEncode({
            'transfer_id': 't1', 'from_id': 'c', 'to_id': 'dev1',
            'file_name': 'hello.txt', 'size_bytes': 5,
            'sha256': '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824',
            'total_chunks': 1, 'received_chunks': [0], 'status': 'complete',
          }),
          200,
        );
      }
      return http.Response('not found', 404);
    });
    final client = ApiClient(config: config, client: mock);
    final view = await client.sendBytes(
        toId: 'dev1', fileName: 'hello.txt', bytes: utf8.encode('hello'));
    expect(view.status, 'complete');
    expect(paths, [
      'POST /transfers',
      'PUT /transfers/t1/chunks/0',
      'POST /transfers/t1/complete',
    ]);
  });

  test('downloadBytes assembles chunks and verifies the hash', () async {
    const view = TransferView(
      transferId: 't9',
      fromId: 'dev1',
      toId: 'me',
      fileName: 'hello.txt',
      sizeBytes: 5,
      sha256: '2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824',
      totalChunks: 1,
      receivedChunks: [0],
      status: 'complete',
    );
    final mock = MockClient((request) async {
      expect(request.url.path, '/transfers/t9/chunks/0');
      return http.Response.bytes(utf8.encode('hello'), 200);
    });
    final client = ApiClient(config: config, client: mock);
    final bytes = await client.downloadBytes(view);
    expect(utf8.decode(bytes), 'hello');

    final badMock = MockClient((_) async => http.Response.bytes(utf8.encode('HELLO'), 200));
    final badClient = ApiClient(config: config, client: badMock);
    expect(badClient.downloadBytes(view), throwsA(isA<AbhiverseApiException>()));
  });

}
