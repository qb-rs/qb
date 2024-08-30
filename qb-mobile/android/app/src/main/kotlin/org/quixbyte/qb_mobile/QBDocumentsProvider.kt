package org.quixbyte.qb_mobile

import android.database.Cursor
import android.database.MatrixCursor
import android.os.CancellationSignal
import android.os.ParcelFileDescriptor
import android.provider.DocumentsContract.Document
import android.provider.DocumentsContract.Root
import android.provider.DocumentsProvider


class QBDocumentsProvider : DocumentsProvider() {
    var DEFAULT_ROOT_PROJECTION =
        arrayOf(
            Root.COLUMN_ROOT_ID, Root.COLUMN_MIME_TYPES,
            Root.COLUMN_FLAGS, Root.COLUMN_ICON, Root.COLUMN_TITLE,
            Root.COLUMN_SUMMARY, Root.COLUMN_DOCUMENT_ID,
            Root.COLUMN_AVAILABLE_BYTES,
        )
    var DEFAULT_DOCUMENT_PROJECTION = arrayOf(
        Document.COLUMN_DOCUMENT_ID, Document.COLUMN_MIME_TYPE,
        Document.COLUMN_DISPLAY_NAME, Document.COLUMN_LAST_MODIFIED,
        Document.COLUMN_FLAGS, Document.COLUMN_SIZE,
    )


    override fun queryRoots(projection: Array<out String?>?): Cursor? {
        var result =
            MatrixCursor(DEFAULT_ROOT_PROJECTION);

        // It's possible to have multiple roots (e.g. for multiple accounts in the
        // same app) -- just add multiple cursor rows.
        var row = result.newRow();
        row.add(Root.COLUMN_ROOT_ID, "qb_mobile")

        // You can provide an optional summary, which helps distinguish roots
        // with the same title. You can also use this field for displaying an
        // user account name.
        row.add(Root.COLUMN_SUMMARY, "a service for quickly synchronizing files")

        // FLAG_SUPPORTS_CREATE means at least one directory under the root supports
        // creating documents. FLAG_SUPPORTS_RECENTS means your application's most
        // recently used documents will show up in the "Recents" category.
        // FLAG_SUPPORTS_SEARCH allows users to search all documents the application
        // shares.
        row.add(
            Root.COLUMN_FLAGS,
            Root.FLAG_SUPPORTS_CREATE or Root.FLAG_SUPPORTS_RECENTS or Root.FLAG_SUPPORTS_SEARCH)

        // COLUMN_TITLE is the root title (e.g. Gallery, Drive).
        row.add(Root.COLUMN_TITLE, "QuixByte")

        // This document id cannot change after it's shared.
        row.add(Root.COLUMN_DOCUMENT_ID, 0);

        // The child MIME types are used to filter the roots and only present to the
        // user those roots that contain the desired type somewhere in their file hierarchy.
        row.add(Root.COLUMN_MIME_TYPES, "");
        row.add(Root.COLUMN_AVAILABLE_BYTES, 100000);
        row.add(Root.COLUMN_ICON, "");

        return result
    }

    override fun queryDocument(
        documentId: String?,
        projection: Array<out String?>?
    ): Cursor? {
        TODO("Not yet implemented")
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
        return true
    }
}