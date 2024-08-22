import 'dart:io';
import 'dart:ui';
import 'dart:async';
import 'package:flutter/services.dart';
import 'package:path_provider/path_provider.dart';
import 'package:process/process.dart';

import 'package:flutter/material.dart';
import 'package:flutter_background_service/flutter_background_service.dart';


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
  final cacheDir = await getApplicationCacheDirectory();
  final file = File('${cacheDir.path}/bin/qb-daemon');
  //if(!await file.exists()) {
    await copyBinary(file);
  //}

  final process = await processManager.start([file.path, '--no-ipc --std']);
  process.stdout.listen((data) {
    print("recv: $data");
  });

  service.on("stop").listen((event) {
    service.stopSelf();
    print("background process is now stopped");
  });

  service.on("start").listen((event) {
    print("background service started!");
  });

  Timer.periodic(const Duration(seconds: 1), (timer) {
    print("service is successfully running ${DateTime.now().second}");
  });
}

Future<void> copyBinary(File dst) async {
  await dst.create(recursive: true);
  final src = await rootBundle.load("assets/bin/qb-daemon");
  await dst.writeAsBytes(src.buffer.asUint8List(src.offsetInBytes, src.lengthInBytes));
  print(src.lengthInBytes);
  
  final chmodProc = await processManager.run(['chmod', '+x', dst.path], runInShell: true);
  print(chmodProc.stdout);

  final fileProc = await processManager.run(['file', dst.path]);
  print(fileProc.stdout);
}
