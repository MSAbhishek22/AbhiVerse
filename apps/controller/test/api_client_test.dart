import 'dart:convert';

import 'package:abhiverse_controller/core/api_client.dart';
import 'package:abhiverse_controller/core/controller_identity.dart';
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
    final identity =
        await ControllerIdentity.fromSeed(List.filled(32, 7), name: 'Pixel');
    final mock = MockClient((request) async {
      final body = jsonDecode(request.body) as Map<String, dynamic>;
      expect(body['code'], '123456');
      final controller = body['controller'] as Map<String, dynamic>;
      expect(controller['name'], 'Pixel');
      expect(controller['device_id'], identity.deviceId);
      expect(controller['kind'], 'controller');
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
    final client = ApiClient(config: config, client: mock, identity: identity);
    final claim = await client.claimPairing(code: '123456', controllerName: 'Pixel');
    expect(claim.agentName, 'My Laptop');
    expect(claim.status, 'claimed');
  });

  test('claimPairing maps 404 to a friendly exception', () async {
    final identity = await ControllerIdentity.fromSeed(List.filled(32, 9));
    final mock = MockClient((_) async => http.Response('{"error":"x"}', 404));
    final client = ApiClient(config: config, client: mock, identity: identity);
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
        expect(request.headers['x-abhiverse-signature']?.length, 128);
        expect(request.headers['x-abhiverse-timestamp'], isNotNull);
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
    final identity = await ControllerIdentity.fromSeed(List.filled(32, 3));
    final client = ApiClient(config: config, client: mock, identity: identity);
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

  test('session signalling: rtc config, create, fetch, end', () async {
    final mock = MockClient((request) async {
      if (request.url.path == '/config/rtc') {
        return http.Response(
          jsonEncode({
            'ice_servers': [
              {'urls': ['stun:stun.l.google.com:19302']}
            ]
          }),
          200,
        );
      }
      if (request.url.path == '/sessions' && request.method == 'POST') {
        final body = jsonDecode(request.body) as Map<String, dynamic>;
        expect(request.headers['x-abhiverse-signature']?.length, 128);
        expect(body['controller_id'], isNot('c1'));
        expect(body['sdp_offer'], 'offer-sdp');
        return http.Response(
          jsonEncode({
            'session_id': 's1',
            'device_id': body['device_id'],
            'controller_id': body['controller_id'],
            'status': 'offered',
            'sdp_offer': 'offer-sdp',
            'sdp_answer': null,
            'created_at': 1,
            'expires_at': 121,
          }),
          201,
        );
      }
      if (request.url.path == '/sessions/s1') {
        return http.Response(
          jsonEncode({
            'session_id': 's1', 'device_id': 'dev1', 'controller_id': 'c',
            'status': 'answered', 'sdp_offer': 'offer-sdp',
            'sdp_answer': 'answer-sdp', 'created_at': 1, 'expires_at': 121,
          }),
          200,
        );
      }
      if (request.url.path == '/sessions/s1/end') {
        return http.Response(
          jsonEncode({
            'session_id': 's1', 'device_id': 'dev1', 'controller_id': 'c',
            'status': 'ended', 'sdp_offer': 'offer-sdp',
            'sdp_answer': 'answer-sdp', 'created_at': 1, 'expires_at': 121,
          }),
          200,
        );
      }
      return http.Response('nf', 404);
    });
    final identity = await ControllerIdentity.fromSeed(List.filled(32, 5));
    final client = ApiClient(config: config, client: mock, identity: identity);
    final servers = await client.fetchRtcConfig();
    expect(servers.single.urls.single, startsWith('stun:'));
    final created = await client.createSession(deviceId: 'dev1', sdpOffer: 'offer-sdp');
    expect(created.status, 'offered');
    final fetched = await client.fetchSession('s1');
    expect(fetched.sdpAnswer, 'answer-sdp');
    final ended = await client.endSession('s1');
    expect(ended.status, 'ended');
  });

}
