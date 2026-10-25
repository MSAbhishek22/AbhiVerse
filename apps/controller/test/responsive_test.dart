import 'dart:convert';

import 'package:abhiverse_controller/core/api_client.dart';
import 'package:abhiverse_controller/core/config.dart';
import 'package:abhiverse_controller/core/health_client.dart';
import 'package:abhiverse_controller/features/home/home_screen.dart';
import 'package:abhiverse_controller/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';

const _config = AbhiverseEndpointConfig(apiBaseUrl: 'http://192.168.1.20:8080');

HealthClient fakeHealth() => HealthClient(
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

ApiClient fakeApi() => ApiClient(
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
    if (request.url.path == '/transfers') {
      return http.Response(jsonEncode({'transfers': []}), 200);
    }
    return http.Response('not found', 404);
  }),
  controllerId: 'test-controller',
);

Future<void> setSize(WidgetTester tester, Size size) async {
  tester.view.physicalSize = size;
  tester.view.devicePixelRatio = 1.0;
  addTearDown(tester.view.resetPhysicalSize);
  addTearDown(tester.view.resetDevicePixelRatio);
}

void main() {
  const sizes = {
    'small phone 320x568': Size(320, 568),
    'phone 360x740': Size(360, 740),
    'large phone 412x915': Size(412, 915),
    'tablet portrait 600x1024': Size(600, 1024),
    'landscape 1024x600': Size(1024, 600),
    'tablet landscape 1280x800': Size(1280, 800),
  };
  for (final entry in sizes.entries) {
    testWidgets('home renders without overflow on ${entry.key}', (
      tester,
    ) async {
      await setSize(tester, entry.value);
      await tester.pumpWidget(
        AbhiverseApp(healthClient: fakeHealth(), apiClient: fakeApi()),
      );
      await tester.pumpAndSettle();
      expect(find.text('ABHIVERSE'), findsWidgets);
      expect(find.text('My Laptop'), findsOneWidget);
      // A RenderFlex overflow surfaces here as a recorded error.
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets('pair dialog fits a small phone', (tester) async {
    await setSize(tester, const Size(320, 568));
    await tester.pumpWidget(
      AbhiverseApp(healthClient: fakeHealth(), apiClient: fakeApi()),
    );
    await tester.pumpAndSettle();
    await tester.ensureVisible(find.text('Pair a device'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Pair a device'));
    await tester.pumpAndSettle();
    expect(find.text('Pairing code'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('settings server dialog validates and saves', (tester) async {
    String? saved;
    var saveCalls = 0;
    await tester.pumpWidget(
      MaterialApp(
        home: HomeScreen(
          healthClient: fakeHealth(),
          apiClient: fakeApi(),
          onEndpointChanged: (url) async {
            saved = url;
            saveCalls++;
          },
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byIcon(Icons.settings));
    await tester.pumpAndSettle();
    expect(find.text('Settings'), findsOneWidget);
    await tester.tap(find.text('Change server'));
    await tester.pumpAndSettle();
    expect(find.text('Server settings'), findsOneWidget);

    // A public http URL must be refused inside the dialog.
    await tester.enterText(find.byType(TextField), 'http://8.8.8.8:8080');
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(find.textContaining('Refusing'), findsOneWidget);
    expect(saveCalls, 0);

    // An https URL saves and closes the dialog, back on Settings.
    await tester.enterText(
      find.byType(TextField),
      'https://abhiverse.example:443',
    );
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(find.text('Server settings'), findsNothing);
    expect(saved, 'https://abhiverse.example:443');
    expect(find.text('https://abhiverse.example:443'), findsOneWidget);

    // Reset passes null back.
    await tester.tap(find.text('Change server'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Reset to default'));
    await tester.pumpAndSettle();
    expect(saved, isNull);
    expect(find.text('Default server (build setting)'), findsOneWidget);
  });
}
