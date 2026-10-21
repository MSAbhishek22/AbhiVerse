import 'package:flutter/material.dart';

import 'core/config.dart';
import 'core/health_client.dart';
import 'features/home/home_screen.dart';

void main() {
  final config = AbhiverseEndpointConfig.fromEnvironment();
  final client = HealthClient(config: config);
  runApp(AbhiverseApp(healthClient: client));
}

class AbhiverseApp extends StatelessWidget {
  const AbhiverseApp({super.key, required this.healthClient});

  final HealthClient healthClient;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'ABHIVERSE',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.indigo),
        useMaterial3: true,
      ),
      home: HomeScreen(healthClient: healthClient),
    );
  }
}
