import 'package:flutter/material.dart';

import '../../core/api_client.dart';
import '../../core/health_client.dart';

enum AbhiverseConnectionState { loading, connected, unreachable }

/// Home screen: branding, live connection status against the
/// coordination API, the paired-device list with presence
/// (Tasks 002–004), and pairing by 6-digit code (Task 003).
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.healthClient, this.apiClient});

  final HealthClient healthClient;
  final ApiClient? apiClient;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  AbhiverseConnectionState _state = AbhiverseConnectionState.loading;
  HealthResponse? _health;
  String? _error;
  List<DeviceView> _devices = const [];
  String? _devicesError;
  String? _pairingMessage;

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
      await _loadDevices();
    } on AbhiverseApiException catch (e) {
      if (!mounted) return;
      setState(() {
        _error = e.message;
        _state = AbhiverseConnectionState.unreachable;
      });
    }
  }

  Future<void> _loadDevices() async {
    final api = widget.apiClient;
    if (api == null) return;
    try {
      final devices = await api.fetchDevices();
      if (!mounted) return;
      setState(() {
        _devices = devices;
        _devicesError = null;
      });
    } on AbhiverseApiException catch (e) {
      if (!mounted) return;
      setState(() => _devicesError = e.message);
    }
  }

  Future<void> _showPairDialog() async {
    final codeController = TextEditingController();
    final nameController = TextEditingController(text: 'My Phone');
    final result = await showDialog<(String, String)>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Pair a device'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Text(
              'Run `abhiverse-agent --pair` on your computer and enter '
              'the 6-digit code it shows.',
            ),
            const SizedBox(height: 12),
            TextField(
              controller: codeController,
              keyboardType: TextInputType.number,
              maxLength: 6,
              decoration: const InputDecoration(labelText: 'Pairing code'),
            ),
            TextField(
              controller: nameController,
              decoration: const InputDecoration(labelText: 'This phone\'s name'),
            ),
          ],
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(
              context,
              (codeController.text.trim(), nameController.text.trim()),
            ),
            child: const Text('Pair'),
          ),
        ],
      ),
    );
    if (result == null || !mounted) return;
    final (code, name) = result;
    final api = widget.apiClient;
    if (api == null || code.length != 6) {
      setState(() => _pairingMessage = 'Enter the 6-digit code from the computer.');
      return;
    }
    try {
      final claim = await api.claimPairing(
        code: code,
        controllerName: name.isEmpty ? 'My Phone' : name,
      );
      if (!mounted) return;
      setState(() => _pairingMessage =
          'Request sent to ${claim.agentName}. Approve it on the computer to finish pairing.');
      await _loadDevices();
    } on AbhiverseApiException catch (e) {
      if (!mounted) return;
      setState(() => _pairingMessage = e.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: const Text('ABHIVERSE')),
      body: Center(
        child: SingleChildScrollView(
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
              if (_state == AbhiverseConnectionState.connected) ...[
                const SizedBox(height: 24),
                _devicesSection(context),
              ],
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

  Widget _devicesSection(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          mainAxisAlignment: MainAxisAlignment.spaceBetween,
          children: [
            Text('Your devices', style: Theme.of(context).textTheme.titleMedium),
            IconButton(
              onPressed: _loadDevices,
              icon: const Icon(Icons.refresh),
              tooltip: 'Refresh devices',
            ),
          ],
        ),
        if (_devicesError != null)
          Text(_devicesError!, textAlign: TextAlign.center)
        else if (_devices.isEmpty)
          const Text(
            'No paired devices yet. Pair your computer to see it here.',
            textAlign: TextAlign.center,
          )
        else
          ..._devices.map(
            (d) => ListTile(
              leading: Icon(
                Icons.circle,
                size: 12,
                color: d.online ? Colors.green : Colors.grey,
              ),
              title: Text(d.name),
              subtitle: Text(
                '${d.platform} · ${d.online ? 'Online' : 'Offline'}',
              ),
            ),
          ),
        const SizedBox(height: 12),
        ElevatedButton.icon(
          onPressed: _showPairDialog,
          icon: const Icon(Icons.link),
          label: const Text('Pair a device'),
        ),
        if (_pairingMessage != null) ...[
          const SizedBox(height: 8),
          Text(_pairingMessage!, textAlign: TextAlign.center),
        ],
      ],
    );
  }
}
