import 'package:abhiverse_controller/core/app_info.dart';
import 'package:abhiverse_controller/features/settings/settings_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> openSettings(WidgetTester tester) async {
  await tester.pumpWidget(
    MaterialApp(
      home: SettingsScreen(
        currentServerUrl: 'http://192.168.1.20:8080',
        onServerChanged: (_) async {},
      ),
    ),
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('settings hub shows server, docs, support, version', (
    tester,
  ) async {
    await openSettings(tester);
    expect(find.text('Settings'), findsOneWidget);
    expect(find.text('http://192.168.1.20:8080'), findsOneWidget);
    expect(find.text('Terms & Conditions'), findsOneWidget);
    expect(find.text('Privacy & Security'), findsOneWidget);
    expect(find.text('FAQ'), findsOneWidget);
    expect(find.text('Open-source licences'), findsOneWidget);
    // Support + About live below the fold; scroll them into view.
    await tester.scrollUntilVisible(find.text(kSupportUpiId), 200);
    expect(find.textContaining('Made with love'), findsOneWidget);
    expect(find.text(kSupportUpiId), findsOneWidget);
    await tester.scrollUntilVisible(
      find.text('ABHIVERSE version $kAppVersion'),
      200,
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('terms screen states the liability bluntly', (tester) async {
    await openSettings(tester);
    await tester.tap(find.text('Terms & Conditions'));
    await tester.pumpAndSettle();
    expect(find.text('Terms & Conditions'), findsWidgets);
    expect(find.text('The blunt part — your responsibility'), findsOneWidget);
    // The user's own words, kept verbatim in spirit: misconfiguration
    // consequences are the user's fault, not the app's.
    expect(find.textContaining('is your fault'), findsWidgets);
    // Scroll to the body paragraph itself: the heading alone does not
    // guarantee the paragraph below it is built yet.
    await tester.scrollUntilVisible(
      find.textContaining('provided as-is'),
      200,
    );
    expect(find.textContaining('no warranty'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('security screen explains keys and honest limits', (
    tester,
  ) async {
    await openSettings(tester);
    await tester.tap(find.text('Privacy & Security'));
    await tester.pumpAndSettle();
    expect(find.text('Keys stay on your devices'), findsOneWidget);
    await tester.scrollUntilVisible(
      find.text('Honest limits — read this'),
      200,
    );
    expect(find.textContaining('No accounts'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('faq answers the no-account question', (tester) async {
    await openSettings(tester);
    await tester.tap(find.text('FAQ'));
    await tester.pumpAndSettle();
    expect(find.text('Do I need an account?'), findsOneWidget);
    expect(find.textContaining('No signup'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('support copy button exists', (tester) async {
    await openSettings(tester);
    await tester.scrollUntilVisible(find.text('Copy'), 200);
    expect(find.text('Copy'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
