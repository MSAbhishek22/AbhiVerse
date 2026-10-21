import 'dart:convert';
import 'dart:math';

import 'package:crypto/crypto.dart';
import 'package:http/http.dart' as http;

import 'config.dart';
import 'controller_identity.dart';
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
  ApiClient({
    required this.config,
    http.Client? client,
    String? controllerId,
    ControllerIdentity? identity,
  })  : _client = client ?? http.Client(),
        _identity = identity,
        controllerId = identity?.deviceId ??
            controllerId ??
            'controller-${Random.secure().nextInt(1 << 32)}';

  final ControllerIdentity? _identity;

  bool get hasIdentity => _identity != null;

  /// Signature headers for a protocol [message] that embeds
  /// timestamp [ts] — the header and the message must carry the
  /// same timestamp, the API rebuilds the message from the header.
  Future<Map<String, String>> _signatureHeaders(int ts, String message) async {
    final identity = _identity;
    if (identity == null) return const {};
    return {
      'x-abhiverse-timestamp': '$ts',
      'x-abhiverse-signature': await identity.sign(message),
    };
  }

  final AbhiverseEndpointConfig config;
  http.Client _client;

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
    final identity = _identity;
    if (identity == null) {
      throw const AbhiverseApiException(
          'This phone has no identity yet — restart the app to create one.');
    }
    final http.Response response;
    try {
      response = await _client
          .post(
            _uri('/pairing/claim'),
            headers: {'content-type': 'application/json'},
            body: jsonEncode({
              'code': code,
              'controller': identity.deviceInfoJson(nameOverride: controllerName),
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


  /// Create a transfer of [bytes] to device [toId], upload every
  /// chunk, and ask the server to verify the assembled hash.
  /// [onProgress] reports (chunksSent, totalChunks) after each chunk.
  Future<TransferView> sendBytes({
    required String toId,
    required String fileName,
    required List<int> bytes,
    void Function(int sent, int total)? onProgress,
  }) async {
    final digest = sha256.convert(bytes).toString();
    final totalChunks =
        bytes.isEmpty ? 0 : (bytes.length + transferChunkSize - 1) ~/ transferChunkSize;
    final requestId = newRequestId();
    final created = await _postJson(
      '/transfers',
      {
        'from_id': controllerId,
        'to_id': toId,
        'file_name': fileName,
        'size_bytes': bytes.length,
        'sha256': digest,
        'total_chunks': totalChunks,
        'request_id': requestId,
      },
      expected: 201,
      signedHeaders: () async {
        final ts = nowUnix();
        return _signatureHeaders(
          ts,
          transferCreateMessage(
            fromId: controllerId,
            toId: toId,
            sha256Hex: digest,
            timestamp: ts,
            requestId: requestId,
          ),
        );
      },
    );
    final view = TransferView.fromJson(created);
    for (var i = 0; i < totalChunks; i++) {
      final start = i * transferChunkSize;
      final end = (start + transferChunkSize > bytes.length)
          ? bytes.length
          : start + transferChunkSize;
      final chunk = bytes.sublist(start, end);
      final response = await _client
          .put(_uri('/transfers/${view.transferId}/chunks/$i'), body: chunk)
          .timeout(const Duration(seconds: 15));
      if (response.statusCode != 200) {
        throw AbhiverseApiException(
            'Chunk ${i + 1} upload failed (HTTP ${response.statusCode}).');
      }
      onProgress?.call(i + 1, totalChunks);
    }
    final done = await _postJson(
        '/transfers/${view.transferId}/complete', const {},
        expected: 200);
    return TransferView.fromJson(done);
  }

  /// Transfers addressed to this controller.
  Future<List<TransferView>> fetchIncoming() async {
    final http.Response response;
    try {
      response = await _client
          .get(_uri('/transfers?for=$controllerId'))
          .timeout(const Duration(seconds: 5));
    } catch (_) {
      throw const AbhiverseApiException('Could not reach the Abhiverse server.');
    }
    if (response.statusCode != 200) {
      throw AbhiverseApiException(
          'Server returned HTTP ${response.statusCode} for transfers.');
    }
    try {
      final decoded = jsonDecode(response.body) as Map<String, dynamic>;
      final raw = decoded['transfers'] as List<dynamic>;
      return raw
          .map((e) => TransferView.fromJson(e as Map<String, dynamic>))
          .toList();
    } catch (_) {
      throw const AbhiverseApiException('Malformed transfers response.');
    }
  }

  /// Download all chunks of a completed transfer and verify the
  /// SHA-256 locally before returning the bytes. Throws if the
  /// assembled file does not match the declared hash.
  Future<List<int>> downloadBytes(TransferView view) async {
    final out = <int>[];
    for (var i = 0; i < view.totalChunks; i++) {
      final response = await _client
          .get(_uri('/transfers/${view.transferId}/chunks/$i'))
          .timeout(const Duration(seconds: 15));
      if (response.statusCode != 200) {
        throw AbhiverseApiException(
            'Chunk ${i + 1} download failed (HTTP ${response.statusCode}).');
      }
      out.addAll(response.bodyBytes);
    }
    final digest = sha256.convert(out).toString();
    if (digest != view.sha256) {
      throw const AbhiverseApiException(
          'Downloaded file failed its SHA-256 check — not saving it.');
    }
    return out;
  }

  Future<Map<String, dynamic>> _postJson(
    String path,
    Map<String, dynamic> body, {
    required int expected,
    Future<Map<String, String>> Function()? signedHeaders,
  }) async {
    // One retry on transport failure: on lossy networks a pooled
    // connection can die after the server processed the request but
    // before its response arrived ("connection closed before full
    // header"). The retry uses a fresh connection and RE-SIGNS with
    // a fresh timestamp (same request_id): the server's replay
    // protection would reject a byte-identical resend, while a
    // re-signed retry of a create the server never saw still works.
    for (var attempt = 0; attempt < 2; attempt++) {
      final headers = signedHeaders == null
          ? const <String, String>{}
          : await signedHeaders();
      final http.Response response;
      try {
        response = await _client
            .post(
              _uri(path),
              headers: {'content-type': 'application/json', ...headers},
              body: jsonEncode(body),
            )
            .timeout(const Duration(seconds: 10));
      } catch (_) {
        if (attempt == 1) {
          throw const AbhiverseApiException(
              'Could not reach the Abhiverse server.');
        }
        _client.close();
        _client = http.Client();
        await Future<void>.delayed(const Duration(milliseconds: 150));
        continue;
      }
      if (response.statusCode != expected) {
        throw AbhiverseApiException(
            'Server returned HTTP ${response.statusCode} for $path.');
      }
      return jsonDecode(response.body) as Map<String, dynamic>;
    }
    throw StateError('unreachable');
  }


  /// Revoke a paired device using this controller's signature
  /// (the API also accepts the device's own signature).
  Future<void> revokeDevice(String deviceId) async {
    final identity = _identity;
    if (identity == null) {
      throw const AbhiverseApiException('This phone has no identity yet.');
    }
    final ts = nowUnix();
    final signature = await identity
        .sign(revokeMessage(deviceId: deviceId, timestamp: ts));
    final http.Response response;
    try {
      response = await _client
          .delete(
            _uri('/devices/$deviceId'),
            headers: {'content-type': 'application/json'},
            body: jsonEncode({'timestamp': ts, 'signature': signature}),
          )
          .timeout(const Duration(seconds: 5));
    } catch (_) {
      throw const AbhiverseApiException('Could not reach the Abhiverse server.');
    }
    if (response.statusCode != 200) {
      throw AbhiverseApiException(
          'Server returned HTTP ${response.statusCode} for revoke.');
    }
  }

  /// ICE servers this deployment asks peers to use (STUN/TURN).
  Future<List<IceServerConfig>> fetchRtcConfig() async {
    final http.Response response;
    try {
      response = await _client
          .get(_uri('/config/rtc'))
          .timeout(const Duration(seconds: 5));
    } catch (_) {
      throw const AbhiverseApiException('Could not reach the Abhiverse server.');
    }
    if (response.statusCode != 200) {
      throw AbhiverseApiException(
          'Server returned HTTP ${response.statusCode} for RTC config.');
    }
    final decoded = jsonDecode(response.body) as Map<String, dynamic>;
    return (decoded['ice_servers'] as List<dynamic>)
        .map((e) => IceServerConfig.fromJson(e as Map<String, dynamic>))
        .toList();
  }

  /// Ask a paired device for a remote-desktop session, carrying our
  /// WebRTC SDP offer. The device user must approve before answering.
  Future<SessionView> createSession({
    required String deviceId,
    required String sdpOffer,
  }) async {
    final offerHash = sha256.convert(utf8.encode(sdpOffer)).toString();
    final requestId = newRequestId();
    final created = await _postJson(
      '/sessions',
      {
        'device_id': deviceId,
        'controller_id': controllerId,
        'sdp_offer': sdpOffer,
        'request_id': requestId,
      },
      expected: 201,
      signedHeaders: () async {
        final ts = nowUnix();
        return _signatureHeaders(
          ts,
          sessionCreateMessage(
            deviceId: deviceId,
            controllerId: controllerId,
            offerSha256Hex: offerHash,
            timestamp: ts,
            requestId: requestId,
          ),
        );
      },
    );
    return SessionView.fromJson(created);
  }

  Future<SessionView> fetchSession(String sessionId) async {
    final http.Response response;
    try {
      response = await _client
          .get(_uri('/sessions/$sessionId?as=$controllerId'))
          .timeout(const Duration(seconds: 5));
    } catch (_) {
      throw const AbhiverseApiException('Could not reach the Abhiverse server.');
    }
    if (response.statusCode != 200) {
      throw AbhiverseApiException(
          'Server returned HTTP ${response.statusCode} for session.');
    }
    return SessionView.fromJson(
        jsonDecode(response.body) as Map<String, dynamic>);
  }

  /// Trickle one local ICE candidate to the session.
  Future<void> postIceCandidate({
    required String sessionId,
    required String candidate,
    String? sdpMid,
    int? sdpMlineIndex,
  }) async {
    await _postJson('/sessions/$sessionId/ice', {
      'from_id': controllerId,
      'candidate': candidate,
      'sdp_mid': sdpMid,
      'sdp_mline_index': sdpMlineIndex,
    }, expected: 200);
  }

  /// The other side's ICE candidates with seq > [after].
  Future<List<Map<String, dynamic>>> fetchIceCandidates({
    required String sessionId,
    int after = 0,
  }) async {
    final http.Response response;
    try {
      response = await _client
          .get(_uri('/sessions/$sessionId/ice?as=$controllerId&after=$after'))
          .timeout(const Duration(seconds: 5));
    } catch (_) {
      throw const AbhiverseApiException('Could not reach the Abhiverse server.');
    }
    if (response.statusCode != 200) {
      throw AbhiverseApiException(
          'Server returned HTTP ${response.statusCode} for ICE candidates.');
    }
    final decoded = jsonDecode(response.body) as Map<String, dynamic>;
    return (decoded['candidates'] as List<dynamic>)
        .cast<Map<String, dynamic>>();
  }

  Future<SessionView> endSession(String sessionId) async {
    final ended = await _postJson('/sessions/$sessionId/end', {
      'from_id': controllerId,
    }, expected: 200);
    return SessionView.fromJson(ended);
  }

  void close() => _client.close();
}
/// A file transfer as reported by the API (mirrors Rust TransferView).
class TransferView {
  const TransferView({
    required this.transferId,
    required this.fromId,
    required this.toId,
    required this.fileName,
    required this.sizeBytes,
    required this.sha256,
    required this.totalChunks,
    required this.receivedChunks,
    required this.status,
  });

  final String transferId;
  final String fromId;
  final String toId;
  final String fileName;
  final int sizeBytes;
  final String sha256;
  final int totalChunks;
  final List<int> receivedChunks;
  final String status; // receiving | complete | failed

  factory TransferView.fromJson(Map<String, dynamic> json) {
    return TransferView(
      transferId: json['transfer_id'] as String,
      fromId: json['from_id'] as String,
      toId: json['to_id'] as String,
      fileName: json['file_name'] as String,
      sizeBytes: json['size_bytes'] as int,
      sha256: json['sha256'] as String,
      totalChunks: json['total_chunks'] as int,
      receivedChunks: (json['received_chunks'] as List<dynamic>).cast<int>(),
      status: json['status'] as String,
    );
  }
}

/// Chunk size shared with the Rust transfer engine.
const int transferChunkSize = 256 * 1024;
/// A remote-desktop session as reported by the API (mirrors Rust
/// `SessionView`). The SDP/ICE plumbing is used by the WebRTC
/// viewer when it lands (slice 006b).
class SessionView {
  const SessionView({
    required this.sessionId,
    required this.deviceId,
    required this.controllerId,
    required this.status,
    required this.sdpAnswer,
  });

  final String sessionId;
  final String deviceId;
  final String controllerId;
  final String status; // offered | answered | rejected | ended | expired
  final String? sdpAnswer;

  factory SessionView.fromJson(Map<String, dynamic> json) {
    return SessionView(
      sessionId: json['session_id'] as String,
      deviceId: json['device_id'] as String,
      controllerId: json['controller_id'] as String,
      status: json['status'] as String,
      sdpAnswer: json['sdp_answer'] as String?,
    );
  }
}

/// One ICE server entry from `GET /config/rtc`.
class IceServerConfig {
  const IceServerConfig({required this.urls, this.username, this.credential});
  final List<String> urls;
  final String? username;
  final String? credential;

  factory IceServerConfig.fromJson(Map<String, dynamic> json) {
    return IceServerConfig(
      urls: (json['urls'] as List<dynamic>).cast<String>(),
      username: json['username'] as String?,
      credential: json['credential'] as String?,
    );
  }
}
