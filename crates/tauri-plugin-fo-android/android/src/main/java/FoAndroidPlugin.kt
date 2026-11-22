package app.fileodyssey.android

import android.app.Activity
import android.content.Intent
import android.net.wifi.WifiNetworkSuggestion
import android.os.Build
import android.provider.Settings
import androidx.activity.result.ActivityResult
import androidx.activity.result.IntentSenderRequest
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSArray
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.android.gms.common.ConnectionResult
import com.google.android.gms.common.GoogleApiAvailability
import com.google.mlkit.vision.documentscanner.GmsDocumentScannerOptions
import com.google.mlkit.vision.documentscanner.GmsDocumentScanning
import com.google.mlkit.vision.documentscanner.GmsDocumentScanningResult
import java.io.File

@InvokeArg
class ScanArgs {
    /** Folder the app gives for this scan's pages. */
    var dir: String = ""
    /** Most pages; 0 for no limit. */
    var pageLimit: Int = 0
}

@InvokeArg
class WifiArgs {
    var ssid: String = ""
    var password: String = ""
    /** "WPA", "WPA2", "SAE", "WPA3", "WEP", "nopass" or "". */
    var security: String = ""
}

@TauriPlugin
class FoAndroidPlugin(private val activity: Activity) : Plugin(activity) {
    private var pagesDir: File? = null

    @Command
    fun scannerAvailable(invoke: Invoke) {
        val code = GoogleApiAvailability.getInstance().isGooglePlayServicesAvailable(activity)
        val ret = JSObject()
        ret.put("available", code == ConnectionResult.SUCCESS)
        invoke.resolve(ret)
    }

    @Command
    fun scanDocument(invoke: Invoke) {
        val args = invoke.parseArgs(ScanArgs::class.java)
        if (args.dir.isEmpty()) {
            invoke.reject("No folder for the scanned pages", "unavailable")
            return
        }
        pagesDir = File(args.dir)
        val options = GmsDocumentScannerOptions.Builder()
            .setGalleryImportAllowed(true)
            .setResultFormats(GmsDocumentScannerOptions.RESULT_FORMAT_JPEG)
            .setScannerMode(GmsDocumentScannerOptions.SCANNER_MODE_FULL)
        if (args.pageLimit > 0) {
            options.setPageLimit(args.pageLimit)
        }
        GmsDocumentScanning.getClient(options.build())
            .getStartScanIntent(activity)
            .addOnSuccessListener { sender ->
                startIntentSenderForResult(
                    invoke,
                    IntentSenderRequest.Builder(sender).build(),
                    "scanResult"
                )
            }
            .addOnFailureListener { e ->
                invoke.reject(e.message ?: "The document scanner couldn't start", "unavailable")
            }
    }

    @ActivityCallback
    fun scanResult(invoke: Invoke, result: ActivityResult) {
        if (result.resultCode != Activity.RESULT_OK) {
            invoke.reject("cancelled", "cancelled")
            return
        }
        val scan = GmsDocumentScanningResult.fromActivityResultIntent(result.data)
        val pages = scan?.pages ?: emptyList()
        val dir = pagesDir ?: File(activity.filesDir, "scans")
        val out = JSArray()
        try {
            dir.mkdirs()
            pages.forEachIndexed { i, page ->
                val file = File(dir, "page-${i + 1}.jpg")
                val input = activity.contentResolver.openInputStream(page.imageUri)
                    ?: throw IllegalStateException("page ${i + 1} is missing")
                input.use { src -> file.outputStream().use { dst -> src.copyTo(dst) } }
                out.put(file.absolutePath)
            }
        } catch (e: Exception) {
            invoke.reject("Couldn't keep the scanned pages: ${e.message}")
            return
        }
        val ret = JSObject()
        ret.put("pages", out)
        invoke.resolve(ret)
    }

    @Command
    fun addWifi(invoke: Invoke) {
        val args = invoke.parseArgs(WifiArgs::class.java)
        val ret = JSObject()
        val security = args.security.uppercase()
        // Android 11+ adds networks through a system dialog the user confirms;
        // WEP isn't supported there, so it goes to the Wi-Fi settings.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && security != "WEP") {
            val b = WifiNetworkSuggestion.Builder().setSsid(args.ssid)
            when (security) {
                "WPA", "WPA2" -> b.setWpa2Passphrase(args.password)
                "SAE", "WPA3" -> b.setWpa3Passphrase(args.password)
                else -> if (args.password.isNotEmpty()) b.setWpa2Passphrase(args.password)
            }
            val intent = Intent(Settings.ACTION_WIFI_ADD_NETWORKS)
            intent.putParcelableArrayListExtra(Settings.EXTRA_WIFI_NETWORK_LIST, arrayListOf(b.build()))
            activity.startActivity(intent)
            ret.put("mode", "dialog")
        } else {
            activity.startActivity(Intent(Settings.ACTION_WIFI_SETTINGS))
            ret.put("mode", "settings")
        }
        invoke.resolve(ret)
    }
}
