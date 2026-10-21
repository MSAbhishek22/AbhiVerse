import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';

import '../../core/api_client.dart';
import '../../core/health_client.dart';
import 'scan_page.dart';
import '../remote/remote_viewer.dart';

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
  List<TransferView> _incoming = const [];
  String? _devicesError;
  String? _notice;

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
      final incoming = await api.fetchIncoming();
      if (!mounted) return;
      setState(() {
        _devices = devices;
        _incoming = incoming;
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
            const SizedBox(height: 8),
            Align(
              alignment: Alignment.centerLeft,
              child: TextButton.icon(
                icon: const Icon(Icons.qr_code_scanner),
                label: const Text('Scan QR code instead'),
                onPressed: () async {
                  final scanned = await Navigator.of(context).push<String>(
                    MaterialPageRoute(
                      builder: (_) => const ScanPairingPage(),
                    ),
                  );
                  if (scanned != null) {
                    codeController.text = scanned;
                  }
                },
              ),
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
      setState(() => _notice = 'Enter the 6-digit code from the computer.');
      return;
    }
    try {
      final claim = await api.claimPairing(
        code: code,
        controllerName: name.isEmpty ? 'My Phone' : name,
      );
      if (!mounted) return;
      setState(() => _notice =
          'Request sent to ${claim.agentName}. Approve it on the computer to finish pairing.');
      await _loadDevices();
    } on AbhiverseApiException catch (e) {
      if (!mounted) return;
      setState(() => _notice = e.message);
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
                const SizedBox(height: 24),
                _incomingSection(context),
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

  Future<void> _showSendDialog(DeviceView device) async {
    final api = widget.apiClient;
    if (api == null) return;
    final nameController = TextEditingController(text: 'note.txt');
    final contentController = TextEditingController();
    var sending = false;
    var progress = '';
    await showDialog<void>(
      context: context,
      builder: (dialogContext) => StatefulBuilder(
        builder: (context, setDialogState) => AlertDialog(
          title: Text('Send a file to ${device.name}'),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              TextField(
                controller: nameController,
                decoration: const InputDecoration(labelText: 'File name'),
              ),
              TextField(
                controller: contentController,
                maxLines: 4,
                decoration: const InputDecoration(labelText: 'Text content'),
              ),
              if (sending) ...[
                const SizedBox(height: 12),
                Text(progress),
              ],
            ],
          ),
          actions: [
            TextButton(
              onPressed: sending ? null : () => Navigator.pop(dialogContext),
              child: const Text('Cancel'),
            ),
            ElevatedButton(
              onPressed: sending
                  ? null
                  : () async {
                      setDialogState(() {
                        sending = true;
                        progress = 'Preparing…';
                      });
                      try {
                        final bytes = utf8.encode(contentController.text);
                        final view = await api.sendBytes(
                          toId: device.deviceId,
                          fileName: nameController.text.trim().isEmpty
                              ? 'note.txt'
                              : nameController.text.trim(),
                          bytes: bytes,
                          onProgress: (sent, total) => setDialogState(
                              () => progress = 'Chunk $sent of $total sent'),
                        );
                        if (!dialogContext.mounted) return;
                        Navigator.pop(dialogContext);
                        if (!mounted) return;
                        setState(() => _notice = view.status == 'complete'
                            ? 'Sent ${view.fileName} — the server verified its SHA-256 hash.'
                            : 'Transfer finished with status: ${view.status}');
                      } on AbhiverseApiException catch (e) {
                        setDialogState(() {
                          sending = false;
                          progress = e.message;
                        });
                      }
                    },
              child: const Text('Send'),
            ),
          ],
        ),
      ),
    );
  }

  Future<void> _confirmRevoke(DeviceView device) async {
    final api = widget.apiClient;
    if (api == null) return;
    final ok = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text('Revoke ${device.name}?'),
        content: const Text(
          'This phone will no longer be able to send files or start '
          'remote sessions with this device, and the device will '
          'disappear from your list. You can pair again later.',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('Cancel'),
          ),
          ElevatedButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('Revoke'),
          ),
        ],
      ),
    );
    if (ok != true) return;
    try {
      await api.revokeDevice(device.deviceId);
      if (!mounted) return;
      setState(() => _notice = 'Revoked pairing with ${device.name}.');
      _loadDevices();
    } on AbhiverseApiException catch (e) {
      if (!mounted) return;
      setState(() => _notice = e.message);
    }
  }

  Future<void> _downloadIncoming(TransferView view) async {
    final api = widget.apiClient;
    if (api == null) return;
    try {
      final bytes = await api.downloadBytes(view);
      final dir = await getApplicationDocumentsDirectory();
      final file = File('${dir.path}/${view.fileName}');
      await file.writeAsBytes(bytes, flush: true);
      if (!mounted) return;
      setState(() => _notice =
          'Saved & SHA-256 verified: ${view.fileName} (${bytes.length} bytes)');
    } on AbhiverseApiException catch (e) {
      if (!mounted) return;
      setState(() => _notice = e.message);
    }
  }

  Widget _incomingSection(BuildContext context) {
    if (_incoming.isEmpty) return const SizedBox.shrink();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Text('Files for you', style: Theme.of(context).textTheme.titleMedium),
        ..._incoming.map(
          (f) => ListTile(
            leading: const Icon(Icons.insert_drive_file),
            title: Text(f.fileName),
            subtitle: Text('${f.sizeBytes} bytes · ${f.status}'),
            trailing: f.status == 'complete'
                ? IconButton(
                    icon: const Icon(Icons.download),
                    tooltip: 'Download and verify',
                    onPressed: () => _downloadIncoming(f),
                  )
                : null,
          ),
        ),
      ],
    );
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
              trailing: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  IconButton(
                    icon: const Icon(Icons.monitor),
                    tooltip: 'Remote desktop to ${d.name}',
                    onPressed: widget.apiClient == null
                        ? null
                        : () => Navigator.of(context).push(
                              MaterialPageRoute(
                                builder: (_) => RemoteViewerPage(
                                  device: d,
                                  api: widget.apiClient!,
                                ),
                              ),
                            ),
                  ),
                  IconButton(
                    icon: const Icon(Icons.upload_file),
                    tooltip: 'Send a file to ${d.name}',
                    onPressed: () => _showSendDialog(d),
                  ),
                  PopupMenuButton<String>(
                    tooltip: 'Device actions',
                    onSelected: (value) {
                      if (value == 'revoke') _confirmRevoke(d);
                    },
                    itemBuilder: (context) => const [
                      PopupMenuItem(
                        value: 'revoke',
                        child: Text('Revoke pairing'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        const SizedBox(height: 12),
        ElevatedButton.icon(
          onPressed: _showPairDialog,
          icon: const Icon(Icons.link),
          label: const Text('Pair a device'),
        ),
        if (_notice != null) ...[
          const SizedBox(height: 8),
          Text(_notice!, textAlign: TextAlign.center),
        ],
      ],
    );
  }
}
