import 'package:flutter_test/flutter_test.dart';
import 'package:http/testing.dart';
import 'package:abhiverse_controller/core/config.dart';
import 'package:abhiverse_controller/core/health_client.dart';
import 'package:abhiverse_controller/main.dart';

void main() {
  testWidgets('shows branding and unreachable state with retry',
      (tester) async {
    final mock = MockClient((_) async => throw Exception('offline'));
    final client = HealthClient(
      config: const AbhiverseEndpointConfig(apiBaseUrl: 'http://localhost:8080'),
      client: mock,
    );
    await tester.pumpWidget(AbhiverseApp(healthClient: client));
    await tester.pumpAndSettle();
    expect(find.text('ABHIVERSE'), findsWidgets);
    expect(find.text('Server unreachable'), findsOneWidget);
    expect(find.text('Retry'), findsOneWidget);
  });
}
