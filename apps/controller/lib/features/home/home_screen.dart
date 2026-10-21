import 'package:flutter/material.dart';

import '../../core/health_client.dart';

enum AbhiverseConnectionState { loading, connected, unreachable }

/// Task 001 home screen: branding, live connection status against
/// the coordination API, the backend version when reachable, and a
/// clear error + retry action when it is not.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.healthClient});

  final HealthClient healthClient;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  AbhiverseConnectionState _state = AbhiverseConnectionState.loading;
  HealthResponse? _health;
  String? _error;

  @override
  void initState() {
    super.initState();
    _check();
  }

  Future<void> _check() async {
    setState(() {
      _state = AbhiverseConnectionState.loading;
      _error = null;
    });
    try {
      final health = await widget.healthClient.fetchHealth();
      if (!mounted) return;
      setState(() {
        _health = health;
        _state = AbhiverseConnectionState.connected;
      });
    } on AbhiverseApiException catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.message;
        _state = AbhiverseConnectionState.unreachable;
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('ABHIVERSE')),
      body: Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Column(
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              const Icon(Icons.hub, size: 72),
              const SizedBox(height: 16),
              Text(
                'ABHIVERSE',
                style: Theme.of(context).textTheme.headlineMedium,
              ),
              const SizedBox(height: 8),
              const Text(
                'One control centre. Every device. Anywhere.',
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 32),
              _statusCard(context),
            ],
          ),
        ),
      ),
    );
  }

  Widget _statusCard(BuildContext context) {
    switch (_state) {
      case AbhiverseConnectionState.loading:
        return const Column(
          children: [
            CircularProgressIndicator(),
            SizedBox(height: 12),
            Text('Checking connection…'),
          ],
        );
      case AbhiverseConnectionState.connected:
        return Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              children: [
                const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.check_circle, color: Colors.green),
                    SizedBox(width: 8),
                    Text('Connected'),
                  ],
                ),
                const SizedBox(height: 8),
                Text('Backend: ${_health!.service}'),
                Text('Backend version: ${_health!.version}'),
                Text('Protocol: v${_health!.protocolVersion}'),
                const SizedBox(height: 8),
                TextButton(onPressed: _check, child: const Text('Refresh')),
              ],
            ),
          ),
        );
      case AbhiverseConnectionState.unreachable:
        return Card(
          child: Padding(
            padding: const EdgeInsets.all(16),
            child: Column(
              children: [
                const Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(Icons.error, color: Colors.red),
                    SizedBox(width: 8),
                    Text('Server unreachable'),
                  ],
                ),
                const SizedBox(height: 8),
                Text(_error ?? 'Unknown error', textAlign: TextAlign.center),
                const SizedBox(height: 8),
                ElevatedButton(
                  onPressed: _check,
                  child: const Text('Retry'),
                ),
              ],
            ),
          ),
        );
    }
  }
}
