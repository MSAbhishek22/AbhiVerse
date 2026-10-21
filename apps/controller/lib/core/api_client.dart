import 'dart:convert';
import 'dart:math';

import 'package:http/http.dart' as http;

import 'config.dart';
import 'health_client.dart';

/// A paired device as reported by `GET /devices` (mirrors the Rust
/// `DeviceView` in abhiverse-protocol).
class DeviceView {
  const DeviceView({
    required this.deviceId,
    required this.name,
    required this.platform,
    required this.online,
    required this.pairedWithName,
    required this.lastSeen,
  });

  final String deviceId;
  final String name;
  final String platform;
  final bool online;
  final String pairedWithName;
  final int? lastSeen;

  factory DeviceView.fromJson(Map<String, dynamic> json) {
    final info = json['info'];
    if (info is! Map<String, dynamic>) {
      throw const AbhiverseApiException('Malformed device entry.');
    }
    final name = info['name'];
    final deviceId = info['device_id'];
    final platform = info['platform'];
    final online = json['online'];
    final pairedWithName = json['paired_with_name'];
    final lastSeen = json['last_seen'];
    if (name is! String ||
        deviceId is! String ||
        platform is! String ||
        online is! bool ||
        pairedWithName is! String ||
        (lastSeen != null && lastSeen is! int)) {
      throw const AbhiverseApiException('Malformed device entry.');
    }
    return DeviceView(
      deviceId: deviceId,
      name: name,
      platform: platform,
      online: online,
      pairedWithName: pairedWithName,
      lastSeen: lastSeen,
    );
  }
}

/// Result of claiming a pairing code: which agent is waiting for
/// approval on its screen.
class ClaimResult {
  const ClaimResult({required this.agentName, required this.status});
  final String agentName;
  final String status;
}

/// Client for the devices + pairing endpoints of the API.
class ApiClient {
  ApiClient({required this.config, http.Client? client})
      : _client = client ?? http.Client(),
        controllerId = 'controller-${Random.secure().nextInt(1 << 32)}';

  final AbhiverseEndpointConfig config;
  final http.Client _client;

  /// Ephemeral controller ID for this app install session. A stable
  /// identity backed by secure storage lands with controller keys.
  final String controllerId;

  Uri _uri(String path) => Uri.parse('${config.apiBaseUrl}$path');

  Future<List<DeviceView>> fetchDevices() async {
    final http.Response response;
    try {
      response =
          await _client.get(_uri('/devices')).timeout(const Duration(seconds: 5));
    } catch (_) {
      throw const AbhiverseApiException('Could not reach the Abhiverse server.');
    }
    if (response.statusCode != 200) {
      throw AbhiverseApiException(
          'Server returned HTTP ${response.statusCode} for devices.');
    }
    try {
      final decoded = jsonDecode(response.body) as Map<String, dynamic>;
      final raw = decoded['devices'] as List<dynamic>;
      return raw
          .map((e) => DeviceView.fromJson(e as Map<String, dynamic>))
          .toList();
    } catch (_) {
      throw const AbhiverseApiException('Malformed devices response.');
    }
  }

  Future<ClaimResult> claimPairing({
    required String code,
    required String controllerName,
  }) async {
    final http.Response response;
    try {
      response = await _client
          .post(
            _uri('/pairing/claim'),
            headers: {'content-type': 'application/json'},
            body: jsonEncode({
              'code': code,
              'controller_id': controllerId,
              'controller_name': controllerName,
            }),
          )
          .timeout(const Duration(seconds: 5));
    } catch (_) {
      throw const AbhiverseApiException('Could not reach the Abhiverse server.');
    }
    if (response.statusCode == 404) {
      throw const AbhiverseApiException(
          'That code was not found — check it, or ask the computer for a new one.');
    }
    if (response.statusCode == 409) {
      throw const AbhiverseApiException('That code has already been used.');
    }
    if (response.statusCode != 200) {
      throw AbhiverseApiException(
          'Server returned HTTP ${response.statusCode} for pairing.');
    }
    try {
      final decoded = jsonDecode(response.body) as Map<String, dynamic>;
      final agent = decoded['agent'] as Map<String, dynamic>;
      return ClaimResult(
        agentName: agent['name'] as String,
        status: decoded['status'] as String,
      );
    } catch (_) {
      throw const AbhiverseApiException('Malformed pairing response.');
    }
  }

  void close() => _client.close();
}
