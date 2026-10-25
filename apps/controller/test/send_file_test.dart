import 'dart:convert';
import 'package:abhiverse_controller/core/api_client.dart';
import 'package:abhiverse_controller/core/config.dart';
import 'package:abhiverse_controller/core/health_client.dart';
import 'package:abhiverse_controller/main.dart';
import 'package:crypto/crypto.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';

const _config = AbhiverseEndpointConfig(apiBaseUrl: 'http://192.168.1.20:8080');

Map<String, dynamic> transferJson(String status) => {
  'transfer_id': 't1',
  'from_id': 'test-controller',
  'to_id': 'dev1',
  'file_name': 'hello.txt',
  'size_bytes': 2,
  'sha256': sha256.convert(utf8.encode('hi')).toString(),
  'total_chunks': 1,
  'received_chunks': [],
  'status': status,
};

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  // Mock the file_picker platform channel: file_picker 11 invokes
  // the file-type name ('any') and returns a list of file maps.
  // Picking returns one small in-memory file.
  TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
      .setMockMethodCallHandler(
        const MethodChannel('miguelruivo.flutter.plugins.filepicker'),
        (call) async {
          if (call.method == 'any') {
            return [
              {
                'name': 'hello.txt',
                'bytes': Uint8List.fromList(utf8.encode('hi')),
                'size': 2,
                'path': null,
                'identifier': null,
              },
            ];
          }
          return null;
        },
      );

  testWidgets('pick a real file and send it end to end', (tester) async {
    final health = HealthClient(
      config: _config,
      client: MockClient(
        (_) async => http.Response(
          jsonEncode({
            'app': 'ABHIVERSE',
            'service': 'abhiverse-api',
            'status': 'ok',
            'version': '0.5.4',
            'protocol_version': 2,
          }),
          200,
        ),
      ),
    );
    final api = ApiClient(
      config: _config,
      client: MockClient((request) async {
        if (request.url.path == '/devices') {
          return http.Response(
            jsonEncode({
              'devices': [
                {
                  'info': {
                    'device_id': 'dev1',
                    'name': 'My Laptop',
                    'platform': 'windows',
                  },
                  'online': true,
                  'paired_with_name': 'My Phone',
                  'last_seen': null,
                },
              ],
            }),
            200,
          );
        }
        if (request.url.path == '/transfers' && request.method == 'GET') {
          return http.Response(jsonEncode({'transfers': []}), 200);
        }
        if (request.url.path == '/transfers' && request.method == 'POST') {
          return http.Response(jsonEncode(transferJson('receiving')), 201);
        }
        if (request.url.path == '/transfers/t1/chunks/0') {
          return http.Response('', 200);
        }
        if (request.url.path == '/transfers/t1/complete') {
          return http.Response(jsonEncode(transferJson('complete')), 200);
        }
        return http.Response('not found', 404);
      }),
      controllerId: 'test-controller',
    );

    await tester.pumpWidget(AbhiverseApp(healthClient: health, apiClient: api));
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.byIcon(Icons.upload_file));
    await tester.pumpAndSettle();
    await tester.tap(find.byIcon(Icons.upload_file));
    await tester.pumpAndSettle();
    expect(find.text('Send a file to My Laptop'), findsOneWidget);

    await tester.tap(find.text('Pick a file'));
    await tester.pumpAndSettle();
    expect(find.text('Picked: hello.txt'), findsOneWidget);

    await tester.tap(find.text('Send'));
    await tester.pumpAndSettle();
    expect(
      find.text('Sent hello.txt — the server verified its SHA-256 hash.'),
      findsOneWidget,
    );
    expect(tester.takeException(), isNull);
  });
}
