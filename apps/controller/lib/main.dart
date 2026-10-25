import 'package:flutter/material.dart';

import 'core/api_client.dart';
import 'core/config.dart';
import 'core/controller_identity.dart';
import 'core/endpoint_store.dart';
import 'core/health_client.dart';
import 'features/home/home_screen.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  // A server URL the user saved in the app's settings wins over the
  // build-time default (which only fits the Android emulator). A
  // release APK is useless if its server address cannot change.
  final stored = await EndpointStore().load();
  final config = stored != null && stored.isNotEmpty
      ? AbhiverseEndpointConfig(apiBaseUrl: stored)
      : AbhiverseEndpointConfig.fromEnvironment();
  // The phone's own device identity (Task 007): created once and
  // kept in secure storage; its key signs controller requests.
  final identity = await ControllerIdentity.loadOrCreate();
  runApp(AbhiverseApp(initialConfig: config, identity: identity));
}

class AbhiverseApp extends StatefulWidget {
  const AbhiverseApp({
    super.key,
    this.healthClient,
    this.apiClient,
    this.initialConfig,
    this.identity,
  });

  /// Test seam: a prebuilt health client (widget tests pass a mock).
  final HealthClient? healthClient;

  /// Test seam: a prebuilt API client.
  final ApiClient? apiClient;

  /// The server to connect to on launch (stored setting, else build
  /// default). Ignored when [healthClient] is supplied.
  final AbhiverseEndpointConfig? initialConfig;

  /// The phone's identity. Server settings are only offered when
  /// the identity is present, because changing server means
  /// rebuilding the signed API client around it.
  final ControllerIdentity? identity;

  @override
  State<AbhiverseApp> createState() => _AbhiverseAppState();
}

class _AbhiverseAppState extends State<AbhiverseApp> {
  late HealthClient _health;
  ApiClient? _api;

  @override
  void initState() {
    super.initState();
    final config =
        widget.initialConfig ??
        widget.healthClient?.config ??
        const AbhiverseEndpointConfig(apiBaseUrl: 'http://10.0.2.2:8080');
    _health = widget.healthClient ?? HealthClient(config: config);
    _api =
        widget.apiClient ??
        (widget.identity != null
            ? ApiClient(config: config, identity: widget.identity)
            : null);
  }

  /// Apply a new server URL (or null to reset to the build default)
  /// and rebuild both clients around it. No app restart needed.
  Future<void> _changeEndpoint(String? url) async {
    final identity = widget.identity;
    if (identity == null) return;
    final store = EndpointStore();
    final config = url == null
        ? AbhiverseEndpointConfig.fromEnvironment()
        : AbhiverseEndpointConfig(apiBaseUrl: url);
    if (url == null) {
      await store.clear();
    } else {
      await store.save(url);
    }
    if (!mounted) return;
    _health.close();
    _api?.close();
    setState(() {
      _health = HealthClient(config: config);
      _api = ApiClient(config: config, identity: identity);
    });
  }

  @override
  void dispose() {
    if (widget.healthClient == null) _health.close();
    if (widget.apiClient == null) _api?.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'ABHIVERSE',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.indigo),
        useMaterial3: true,
      ),
      home: HomeScreen(
        healthClient: _health,
        apiClient: _api,
        onEndpointChanged: widget.identity == null ? null : _changeEndpoint,
      ),
    );
  }
}
