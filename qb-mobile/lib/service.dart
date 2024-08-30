import 'dart:convert';
import 'dart:ui';
import 'dart:async';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:process/process.dart';

import 'package:flutter/material.dart';
import 'package:flutter_background_service/flutter_background_service.dart';
import 'package:qb_mobile/src/rust/api/daemon.dart';
import 'package:qb_mobile/src/rust/api/log.dart';
import 'package:qb_mobile/src/rust/frb_generated.dart';

const ProcessManager processManager = LocalProcessManager();

const methodChannel =
    MethodChannel('org.quixbyte.qb_mobile/android_documents_provider');

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
  await RustLib.init();
  initLog().listen((msg) => print(utf8.decode(msg)));

  final dir = await getApplicationDocumentsDirectory();
  final daemon = await DaemonWrapper.init(path: dir.path);

  service.on("stop").listen((event) {
    service.stopSelf();
    print("background process is now stopped");
  });

  service.on("start").listen((event) {
    print("background service started!");
  });

  final percentage = await methodChannel.invokeMethod('kekw');
  print("Battery percentage: $percentage");

  //Timer.periodic(const Duration(seconds: 1), (timer) {
  //  print("service is successfully running ${DateTime.now().second}");
  //});

  while (true) {
    await daemon.process();
  }
}

//Future<void> copyBinary(String srcBin, File dst) async {
//  await dst.create(recursive: true);
//  final src = await rootBundle.load("assets/bin/$srcBin");
//  await dst.writeAsBytes(src.buffer.asUint8List(src.offsetInBytes, src.lengthInBytes));
//  print(src.lengthInBytes);
//
//  final chmodProc = await processManager.run(['chmod', '+x', dst.path], runInShell: true);
//  print(chmodProc.stdout);
//  print(chmodProc.stderr);
//
//  final fileProc = await processManager.run(['ls', '-la', dst.path]);
//  print(fileProc.stdout);
//}
