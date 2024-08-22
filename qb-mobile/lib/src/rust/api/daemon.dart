// This file is automatically generated, so please do not edit it.
// Generated by `flutter_rust_bridge`@ 2.3.0.

// ignore_for_file: invalid_use_of_internal_member, unused_import, unnecessary_import

import '../frb_generated.dart';
import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart';

// These functions are ignored because they are not marked as `pub`: `_process`

// Rust type: RustOpaqueMoi<flutter_rust_bridge::for_generated::RustAutoOpaqueInner<DaemonWrapper>>
abstract class DaemonWrapper implements RustOpaqueInterface {
  /// Cancel processing the daemon.
  Future<void> cancel();

  /// Initialize a new daemon process.
  static Future<DaemonWrapper> init({required String path}) =>
      RustLib.instance.api.crateApiDaemonDaemonWrapperInit(path: path);

  /// Process the daemon. This can be canceled using the cancel method.
  /// If called twice, this will cancel the previous execution.
  Future<void> process();
}
