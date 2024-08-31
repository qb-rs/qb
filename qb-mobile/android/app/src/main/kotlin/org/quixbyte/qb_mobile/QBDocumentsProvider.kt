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
import io.flutter.embedding.engine.dart.DartExecutor
import io.flutter.plugin.common.JSONMethodCodec
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import io.flutter.util.PathUtils
import java.io.File
import java.util.concurrent.atomic.AtomicBoolean

class QBDocumentsProvider : DocumentsProvider(), MethodChannel.MethodCallHandler {
    private var isInit: AtomicBoolean = AtomicBoolean(false)
    private lateinit var channel: MethodChannel
    private lateinit var filesDir: File

    // Constants
    private val TAG = "QBDocumentsProvider"
    private val ENGINE_ID = "org.quixbyte.qb_mobile/documents_provider"
    private val CHANNEL_ID = "org.quixbyte.qb_mobile/documents_provider"
    private val DOCUMENT_ID_PREFIX = "org.quixbyte.qb_mobile/"

    var DEFAULT_ROOT_PROJECTION = arrayOf(
        Root.COLUMN_ROOT_ID,
        Root.COLUMN_DOCUMENT_ID,
        Root.COLUMN_TITLE,
        Root.COLUMN_SUMMARY,
        Root.COLUMN_FLAGS,
        Root.COLUMN_ICON,
    )
    var DEFAULT_DOCUMENT_PROJECTION = arrayOf(
        Document.COLUMN_DOCUMENT_ID,
        Document.COLUMN_DISPLAY_NAME,
        Document.COLUMN_MIME_TYPE,
        Document.COLUMN_FLAGS,
        Document.COLUMN_SIZE,
        Document.COLUMN_LAST_MODIFIED,
    )

    fun idToFile(id: String): File {
        return filesDir.resolve(id.substring(DOCUMENT_ID_PREFIX.length))
    }

    override fun queryRoots(projection: Array<out String?>?): Cursor? {
        Log.i(TAG, "querying roots")

        var projection = projection ?: DEFAULT_ROOT_PROJECTION
        var cursor = MatrixCursor(projection)

        cursor.newRow().apply {
            add(Root.COLUMN_DOCUMENT_ID, "$DOCUMENT_ID_PREFIX.")
            add(Root.COLUMN_TITLE, "QuixByte")
            add(Root.COLUMN_SUMMARY, "your files")
            add(
                Root.COLUMN_FLAGS,
                Root.FLAG_SUPPORTS_CREATE or Root.FLAG_SUPPORTS_RECENTS or Root.FLAG_SUPPORTS_SEARCH
            )
            add(Root.COLUMN_ICON, R.drawable.ic_launcher_round)
        }

        return cursor;
    }

    override fun queryDocument(documentId: String?, projection: Array<out String?>?): Cursor? {
        Log.i(TAG, "querying document '$documentId'")

        if (documentId == null) {
            TODO("unimplemented")
        }

        var projection = projection ?: DEFAULT_DOCUMENT_PROJECTION
        var cursor = MatrixCursor(projection)

        var file = idToFile(documentId)

        cursor.newRow().apply {
            add(Document.COLUMN_DOCUMENT_ID, documentId)
            add(Document.COLUMN_DISPLAY_NAME, file.name)
            var flag = Document.FLAG_DIR_SUPPORTS_CREATE
            flag = flag or Document.FLAG_SUPPORTS_WRITE;
            flag = flag or Document.FLAG_SUPPORTS_DELETE;
            flag = flag or Document.FLAG_SUPPORTS_RENAME;
            add(
                Document.COLUMN_FLAGS, flag
            )
            add(
                Document.COLUMN_MIME_TYPE, getMimeType(file)
            )
            add(
                Document.COLUMN_SIZE, getSize(file)
            )
            add(
                Document.COLUMN_LAST_MODIFIED, file.lastModified()
            )
        }

        return cursor
    }

    fun getMimeType(file: File): String {
        if (file.isDirectory) {
            return Document.MIME_TYPE_DIR
        }

        return "text/plain"
    }

    fun getSize(file: File): Long {
        return if (file.isDirectory) 0 else file.length()
    }

    override fun queryChildDocuments(
        parentDocumentId: String?, projection: Array<out String?>?, sortOrder: String?
    ): Cursor? {
        Log.i(TAG, "querying documents from parent '$parentDocumentId'")

        if (parentDocumentId == null) {
            TODO("unimplemented")
        }

        var projection = projection ?: DEFAULT_DOCUMENT_PROJECTION
        var cursor = MatrixCursor(projection)

        var file = idToFile(parentDocumentId)
        for (it in file.listFiles()!!) {
            cursor.newRow().apply {
                var id = parentDocumentId + "/" + it.name
                add(Document.COLUMN_DOCUMENT_ID, id)
                add(Document.COLUMN_DISPLAY_NAME, it.name)
                var flag = Document.FLAG_DIR_SUPPORTS_CREATE
                flag = flag or Document.FLAG_SUPPORTS_WRITE;
                flag = flag or Document.FLAG_SUPPORTS_DELETE;
                flag = flag or Document.FLAG_SUPPORTS_RENAME;
                add(
                    Document.COLUMN_FLAGS, flag
                )
                add(
                    Document.COLUMN_MIME_TYPE, getMimeType(it)
                )
                add(
                    Document.COLUMN_SIZE, getSize(it)
                )
                add(
                    Document.COLUMN_LAST_MODIFIED, it.lastModified()
                )
            }
        }

        return cursor
    }

    override fun openDocument(
        documentId: String?, mode: String?, signal: CancellationSignal?
    ): ParcelFileDescriptor? {
        TODO("Not yet implemented")
    }

    /**
     * onCreate is called to initialize this documents provider
     */
    override fun onCreate(): Boolean {
        Log.i(TAG, "initializing")

        var context = getContext()
        if (context == null) {
            TODO("Context is null, this should not happen")
        }

        // get the files directory path
        filesDir = File(PathUtils.getDataDirectory(context)).resolve("files");

        Log.i(TAG, "using files directory at ${filesDir.path}")

        // This looks hacky, but works
        Handler(Looper.getMainLooper()).post {
            Log.w(TAG, "starting dart handler...")

            try {
                runDart("init")
            } catch (e: Error) {
                Log.e(TAG, "error while starting dart handler: $e")
            }

            isInit.set(true)
        }

        return true;
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
        var dartEntrypoint = DartExecutor.DartEntrypoint(
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

        channel = MethodChannel(
            executor.getBinaryMessenger(), CHANNEL_ID, JSONMethodCodec.INSTANCE
        )
        channel.setMethodCallHandler(this)

        return engine
    }

    override fun onMethodCall(call: MethodCall, result: MethodChannel.Result) {
        TODO("Not yet implemented")
    }
}
