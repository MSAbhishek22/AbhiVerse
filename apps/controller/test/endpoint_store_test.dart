import 'package:abhiverse_controller/core/endpoint_store.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  final backing = <String, String>{};
  TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
      .setMockMethodCallHandler(
        const MethodChannel('plugins.it_nomads.com/flutter_secure_storage'),
        (call) async {
          final args = (call.arguments as Map?)?.cast<String, dynamic>() ?? {};
          final key = args['key'] as String?;
          switch (call.method) {
            case 'read':
              return backing[key];
            case 'write':
              backing[key!] = args['value'] as String;
              return null;
            case 'delete':
              backing.remove(key);
              return null;
          }
          return null;
        },
      );

  test('round-trips the saved server URL', () async {
    final store = EndpointStore();
    expect(await store.load(), isNull);
    await store.save('http://192.168.1.20:8080');
    expect(await store.load(), 'http://192.168.1.20:8080');
    await store.clear();
    expect(await store.load(), isNull);
  });
}
