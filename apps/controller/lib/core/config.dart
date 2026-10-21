/// Configuration for reaching the ABHIVERSE coordination API.
///
/// The base URL is never hardcoded to a production server. Override
/// it at run/build time:
///
/// ```sh
/// flutter run --dart-define=ABHIVERSE_API_BASE_URL=http://192.168.1.20:8080
/// ```
///
/// Defaults to `http://10.0.2.2:8080`, which is how the Android
/// emulator reaches a server running on the host machine's
/// localhost. On a physical device, pass your computer's LAN IP.
class AbhiverseEndpointConfig {
  const AbhiverseEndpointConfig({required this.apiBaseUrl});

  final String apiBaseUrl;

  factory AbhiverseEndpointConfig.fromEnvironment() {
    const url = String.fromEnvironment(
      'ABHIVERSE_API_BASE_URL',
      defaultValue: 'http://10.0.2.2:8080',
    );
    return const AbhiverseEndpointConfig(apiBaseUrl: url);
  }

  Uri healthUri() => Uri.parse('$apiBaseUrl/health');
}
