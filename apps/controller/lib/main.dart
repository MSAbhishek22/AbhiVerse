import 'package:flutter/material.dart';

import 'core/api_client.dart';
import 'core/config.dart';
import 'core/health_client.dart';
import 'features/home/home_screen.dart';

void main() {
  final config = AbhiverseEndpointConfig.fromEnvironment();
  final client = HealthClient(config: config);
  final apiClient = ApiClient(config: config);
  runApp(AbhiverseApp(healthClient: client, apiClient: apiClient));
}

class AbhiverseApp extends StatelessWidget {
  const AbhiverseApp({super.key, required this.healthClient, this.apiClient});

  final HealthClient healthClient;
  final ApiClient? apiClient;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'ABHIVERSE',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.indigo),
        useMaterial3: true,
      ),
      home: HomeScreen(healthClient: healthClient, apiClient: apiClient),
    );
  }
}
