package org.quixbyte.qb_mobile

import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.Handler
import android.os.Looper
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider
import android.util.Log
import androidx.annotation.UiThread
import io.flutter.FlutterInjector
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.embedding.engine.FlutterEngineCache
import io.flutter.embedding.engine.FlutterJNI
import io.flutter.embedding.engine.dart.DartExecutor
import io.flutter.plugin.common.JSONMethodCodec
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

class QBDocumentsProvider : DocumentsProvider(), MethodChannel.MethodCallHandler {
    private lateinit var channel: MethodChannel
    private var TAG = "QBDocumentsProvider"
    private var ENGINE_ID = "org.quixbyte.qb_mobile/documents_provider"
    private var CHANNEL_ID = "org.quixbyte.qb_mobile/documents_provider"

    var DEFAULT_ROOT_PROJECTION =
        arrayOf(
            Root.COLUMN_ROOT_ID,
            Root.COLUMN_MIME_TYPES,
            Root.COLUMN_FLAGS,
            Root.COLUMN_ICON,
            Root.COLUMN_TITLE,
            Root.COLUMN_SUMMARY,
            Root.COLUMN_DOCUMENT_ID,
            Root.COLUMN_AVAILABLE_BYTES,
        )
    var DEFAULT_DOCUMENT_PROJECTION =
        arrayOf(
            Document.COLUMN_DOCUMENT_ID,
            Document.COLUMN_MIME_TYPE,
            Document.COLUMN_DISPLAY_NAME,
            Document.COLUMN_LAST_MODIFIED,
            Document.COLUMN_FLAGS,
            Document.COLUMN_SIZE,
        )

    override fun queryRoots(projection: Array<out String?>?): Cursor? {
        onCreate()

        var result = MatrixCursor(DEFAULT_ROOT_PROJECTION)

        // It's possible to have multiple roots (e.g. for multiple accounts in the
        // same app) -- just add multiple cursor rows.
        var row = result.newRow()
        row.add(Root.COLUMN_ROOT_ID, "qb_mobile")

        // You can provide an optional summary, which helps distinguish roots
        // with the same title. You can also use this field for displaying an
        // user account name.
        row.add(Root.COLUMN_SUMMARY, "local files")

        // FLAG_SUPPORTS_CREATE means at least one directory under the root supports
        // creating documents. FLAG_SUPPORTS_RECENTS means your application's most
        // recently used documents will show up in the "Recents" category.
        // FLAG_SUPPORTS_SEARCH allows users to search all documents the application
        // shares.
        row.add(
            Root.COLUMN_FLAGS,
            Root.FLAG_SUPPORTS_CREATE or Root.FLAG_SUPPORTS_RECENTS or Root.FLAG_SUPPORTS_SEARCH
        )

        // COLUMN_TITLE is the root title (e.g. Gallery, Drive).
        row.add(Root.COLUMN_TITLE, "QuixByte")

        // This document id cannot change after it's shared.
        row.add(Root.COLUMN_DOCUMENT_ID, 0)

        // The child MIME types are used to filter the roots and only present to the
        // user those roots that contain the desired type somewhere in their file hierarchy.
        row.add(Root.COLUMN_MIME_TYPES, "")
        row.add(Root.COLUMN_AVAILABLE_BYTES, 100000)
        row.add(Root.COLUMN_ICON, "")

        return result
    }

    override fun queryDocument(documentId: String?, projection: Array<out String?>?): Cursor? {
        startDart("onCreate");
        return null;
        //TODO("Not yet implemented")
    }

    override fun queryChildDocuments(
        parentDocumentId: String?,
        projection: Array<out String?>?,
        sortOrder: String?
    ): Cursor? {
        TODO("Not yet implemented")
    }

    override fun openDocument(
        documentId: String?,
        mode: String?,
        signal: CancellationSignal?
    ): ParcelFileDescriptor? {
        TODO("Not yet implemented")
    }

    override fun onCreate(): Boolean {
        startDart("onCreate");

        return true;
    }

    /**
     * Start a task to run a dart entrypoint. It is not guaranteed that the dart
     * entrypoint will be started instantly.
     */
    fun startDart(entrypoint: String) {
        startDart(entrypoint, null)
    }

    /**
     * Start a task to run a dart entrypoint. It is not guaranteed that the dart
     * entrypoint will be started instantly.
     */
    fun startDart(entrypoint: String, dartEntrypointArgs: List<String>?) {
        Handler(Looper.getMainLooper()).post {
            Log.w(TAG, "starting dart entrypoint $entrypoint...")

            try {
                runDart(entrypoint, dartEntrypointArgs)
            } catch (e: Error) {
                Log.e(TAG, "Error while starting dart entrypoint $entrypoint: $e")
            }
        }
    }

    /**
     * Start a dart entrypoint without arguments. This code must
     * be executed in the main thread.
     *
     * See: Handler(Looper.getMainLooper()).post if not on the main thread
     */
    @UiThread
    fun runDart(entrypoint: String) {
        runDart(entrypoint, null)
    }

    /**
     * Start a dart entrypoint with arguments. This code must
     * be executed in the main thread.
     *
     * See: Handler(Looper.getMainLooper()).post if not on the main thread
     */
    @UiThread
    fun runDart(entrypoint: String, dartEntrypointArgs: List<String>?) {
        // get the flutter engine
        var engine = getEngine()

        var flutterLoader = FlutterInjector.instance().flutterLoader()
        var dartEntrypoint =
            DartExecutor.DartEntrypoint(
                flutterLoader.findAppBundlePath(),
                "package:qb_mobile/documents_provider.dart",
                entrypoint
            )

        engine.dartExecutor.executeDartEntrypoint(dartEntrypoint, dartEntrypointArgs)
    }

    /**
     * This will try to get the flutter engine from the cache (if there is one)
     * and otherwise creates a new flutter engine for this context. This code must
     * be executed in the main thread.
     *
     * See: Handler(Looper.getMainLooper()).post if not on the main thread
     */
    @UiThread
    fun getEngine(): FlutterEngine {
        var engineCache = FlutterEngineCache.getInstance();
        if (engineCache.contains(ENGINE_ID)) {
            return engineCache.get(ENGINE_ID)!!;
        }

        var context = getContext()
        if (context == null) {
            TODO("Context is null, this should not happen")
        }

        var injector = FlutterInjector.instance()
        var flutterLoader = injector.flutterLoader()
        // initialize flutter if it's not initialized yet
        if (!flutterLoader.initialized()) {
            flutterLoader.startInitialization(context)
        }
        flutterLoader.ensureInitializationComplete(context, null)

        var engine = FlutterEngine(context)
        var executor = engine.dartExecutor

        var methodChannel =
            MethodChannel(
                executor.getBinaryMessenger(),
                CHANNEL_ID,
                JSONMethodCodec.INSTANCE
            )
        methodChannel.setMethodCallHandler(this)

        return engine
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        TODO("Not yet implemented")
    }
}
