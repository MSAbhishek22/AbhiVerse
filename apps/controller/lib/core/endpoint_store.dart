import 'package:flutter_secure_storage/flutter_secure_storage.dart';

/// Persists the API base URL the user chose in the app's server
/// settings, so a release APK is not stuck with the build-time
/// default (which only fits the Android emulator). The URL is not
/// a secret, but secure storage is already the app's store for
/// device material and keeps it out of plain preferences backups.
///
/// A missing value means "use the build-time default".
class EndpointStore {
  EndpointStore({FlutterSecureStorage? storage})
    : _storage = storage ?? const FlutterSecureStorage();

  static const String key = 'abhiverse_api_base_url';

  final FlutterSecureStorage _storage;

  Future<String?> load() => _storage.read(key: key);

  Future<void> save(String url) => _storage.write(key: key, value: url);

  Future<void> clear() => _storage.delete(key: key);
}
