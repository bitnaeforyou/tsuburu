//! Handing a downloaded APK to the system installer.
//!
//! A phone has no equivalent of writing over the file that is running: an app
//! is replaced by the package installer, which only the operating system may
//! run and which only starts for an intent. So the update ends here rather
//! than in `swap` - the bytes are fetched the same way, and then the installer
//! takes over and asks the reader to confirm.
//!
//! The intent plumbing itself lives in `tsuburu-host`, which the share sheet
//! needs as much as this does.

use std::path::Path;

use jni::JNIEnv;
use jni::objects::{JObject, JValue};
use tsuburu_host::android::{
    GRANT_READ, NEW_TASK, add_flags, application, attached, intent, package_name, start, text, uri,
    uri_for_file,
};

const PACKAGE: &str = "application/vnd.android.package-archive";

/// Opens the installer on `apk`.
///
/// Says whether the installer was opened. A reader who has not allowed this
/// app to install applications gets no installer and no refusal either, so
/// the caller has to be told the difference rather than left to assume the
/// screen came up.
pub fn install(apk: &Path) -> Result<bool, String> {
    attached(|env| ask(env, apk))
}

/// Whether the reader has allowed this app to install applications.
///
/// Worth asking before fetching eighty megabytes, which do not help.
pub fn allowed() -> Result<bool, String> {
    attached(|env| {
        let context = application(env)?;
        may_install(env, &context)
    })
}

/// Opens the one settings screen that can grant it, since an app cannot ask
/// for this permission in a dialogue the way it asks for the camera.
pub fn ask_to_allow() -> Result<(), String> {
    attached(|env| {
        let context = application(env)?;
        let intent = intent(env, "android.settings.MANAGE_UNKNOWN_APP_SOURCES")?;
        let name = package_name(env, &context)?;
        let uri = uri(env, &format!("package:{name}"))?;
        env.call_method(
            &intent,
            "setData",
            "(Landroid/net/Uri;)Landroid/content/Intent;",
            &[JValue::Object(&uri)],
        )
        .map_err(|e| e.to_string())?;
        add_flags(env, &intent, NEW_TASK)?;
        start(env, &context, &intent)
    })
}

fn ask(env: &mut JNIEnv, apk: &Path) -> Result<bool, String> {
    let context = application(env)?;
    if !may_install(env, &context)? {
        return Ok(false);
    }

    let uri = uri_for_file(env, &context, apk)?;
    let intent = intent(env, "android.intent.action.VIEW")?;
    let kind = text(env, PACKAGE)?;
    env.call_method(
        &intent,
        "setDataAndType",
        "(Landroid/net/Uri;Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&uri), JValue::Object(&kind)],
    )
    .map_err(|e| e.to_string())?;
    add_flags(env, &intent, GRANT_READ | NEW_TASK)?;

    start(env, &context, &intent)?;
    Ok(true)
}

fn may_install(env: &mut JNIEnv, context: &JObject) -> Result<bool, String> {
    let packages = env
        .call_method(context, "getPackageManager", "()Landroid/content/pm/PackageManager;", &[])
        .and_then(|value| value.l())
        .map_err(|e| e.to_string())?;
    // Only Android 8 and later asks; below it the permission alone is enough
    // and the method is not there to be called.
    match env.call_method(&packages, "canRequestPackageInstalls", "()Z", &[]) {
        Ok(value) => value.z().map_err(|e| e.to_string()),
        Err(_) => {
            env.exception_clear().ok();
            Ok(true)
        }
    }
}
