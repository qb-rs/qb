import 'dart:convert';
import 'dart:ui';
import 'dart:async';
import 'package:path_provider/path_provider.dart';
import 'package:process/process.dart';

import 'package:flutter/material.dart';
import 'package:flutter_background_service/flutter_background_service.dart';
import 'package:qb_mobile/src/rust/api/daemon.dart';
import 'package:qb_mobile/src/rust/api/log.dart';
import 'package:qb_mobile/src/rust/frb_generated.dart';

const ProcessManager processManager = LocalProcessManager();

Future<void> initializeService() async {
  final service = FlutterBackgroundService();

  await service.configure(
    iosConfiguration: IosConfiguration(
      autoStart: true,
      onForeground: onStart,
      onBackground: onIosBackground,
    ),
    androidConfiguration: AndroidConfiguration(
      autoStart: true,
      onStart: onStart,
      isForegroundMode: false,
      autoStartOnBoot: true,
    ),
  );
}

void startBackgroundService() {
  final service = FlutterBackgroundService();
  service.startService();
}

void stopBackgroundService() {
  final service = FlutterBackgroundService();
  service.invoke("stop");
}

@pragma('vm:entry-point')
Future<bool> onIosBackground(ServiceInstance service) async {
  WidgetsFlutterBinding.ensureInitialized();
  DartPluginRegistrant.ensureInitialized();

  return true;
}

@pragma('vm:entry-point')
void onStart(ServiceInstance service) async {
  WidgetsFlutterBinding.ensureInitialized();

  await RustLib.init();
  initLog().listen((msg) => print(utf8.decode(msg)));

  final dir = await getApplicationDocumentsDirectory();
  final daemon = await DaemonWrapper.init(path: dir.path);

  if ((await daemon.list()).isEmpty) {
    await daemon.add(
      name: "tcp-client",
      contentType: "application/json",
      content: utf8.encode('{"addr":"192.168.178.84:6969","auth":[]}'),
    );
  }

  service.on("stop").listen((event) {
    service.stopSelf();
    print("background process is now stopped");
  });

  service.on("start").listen((event) {
    print("background service started!");
  });

  service.on("notify").listen((event) async {
    print("notify received: $event");
    await daemon.bridge(
        id: BigInt.from(0), data: utf8.encode(event!["data"] as String));
  });

  //Timer.periodic(const Duration(seconds: 1), (timer) {
  //  print("service is successfully running ${DateTime.now().second}");
  //});

  while (true) {
    await daemon.process();
  }
}
