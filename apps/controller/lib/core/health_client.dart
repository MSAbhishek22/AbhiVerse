import 'dart:convert';

import 'package:http/http.dart' as http;

import 'config.dart';

/// Mirror of `HealthResponse` in `crates/protocol` (Rust).
class HealthResponse {
  const HealthResponse({
    required this.app,
    required this.service,
    required this.status,
    required this.version,
    required this.protocolVersion,
  });

  final String app;
  final String service;
  final String status;
  final String version;
  final int protocolVersion;

  factory HealthResponse.fromJson(Map<String, dynamic> json) {
    final app = json['app'];
    final service = json['service'];
    final status = json['status'];
    final version = json['version'];
    final protocolVersion = json['protocol_version'];
    if (app is! String ||
        service is! String ||
        status is! String ||
        version is! String ||
        protocolVersion is! int) {
      throw const AbhiverseApiException('Malformed health response from server.');
    }
    return HealthResponse(
      app: app,
      service: service,
      status: status,
      version: version,
      protocolVersion: protocolVersion,
    );
  }
}

class AbhiverseApiException implements Exception {
  const AbhiverseApiException(this.message);
  final String message;
  @override
  String toString() => message;
}

/// Thin client for the coordination API. The [http.Client] is
/// injectable so tests can use a mock instead of a real network.
class HealthClient {
  HealthClient({required this.config, http.Client? client})
      : _client = client ?? http.Client();

  final AbhiverseEndpointConfig config;
  final http.Client _client;

  Future<HealthResponse> fetchHealth() async {
    final http.Response response;
    try {
      response = await _client
          .get(config.healthUri())
          .timeout(const Duration(seconds: 5));
    } catch (_) {
      throw const AbhiverseApiException(
        'Could not reach the ABHIVERSE server. Check it is running and the '
        'API URL is correct.',
      );
    }
    if (response.statusCode != 200) {
      throw AbhiverseApiException(
        'ABHIVERSE server returned HTTP ${response.statusCode}.',
      );
    }
    try {
      final decoded = jsonDecode(response.body) as Map<String, dynamic>;
      return HealthResponse.fromJson(decoded);
    } catch (_) {
      throw const AbhiverseApiException('Malformed health response from server.');
    }
  }

  void close() => _client.close();
}
