package com.xfarid.proxmox_desktop

import android.app.Activity
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class TokenArgs {
    lateinit var id: String
    var token: String? = null
}

/**
 * API tokens at rest: EncryptedSharedPreferences with an AES-256 master key
 * held in the Android Keystore. Mirrors the desktop OS-keyring behaviour.
 */
@TauriPlugin
class KeystorePlugin(private val activity: Activity) : Plugin(activity) {
    private val prefs by lazy {
        val ctx = activity.applicationContext
        val key = MasterKey.Builder(ctx)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        EncryptedSharedPreferences.create(
            ctx,
            "proxmox-tokens",
            key,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        )
    }

    @Command
    fun getToken(invoke: Invoke) {
        val args = invoke.parseArgs(TokenArgs::class.java)
        val token = prefs.getString(args.id, null)
        if (token == null) {
            invoke.reject("no token stored for connection ${args.id}")
            return
        }
        val ret = JSObject()
        ret.put("token", token)
        invoke.resolve(ret)
    }

    @Command
    fun setToken(invoke: Invoke) {
        val args = invoke.parseArgs(TokenArgs::class.java)
        prefs.edit().putString(args.id, args.token).apply()
        invoke.resolve()
    }

    @Command
    fun deleteToken(invoke: Invoke) {
        val args = invoke.parseArgs(TokenArgs::class.java)
        prefs.edit().remove(args.id).apply()
        invoke.resolve()
    }
}
