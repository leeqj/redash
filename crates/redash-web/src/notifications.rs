use web_sys::{Notification, NotificationOptions, NotificationPermission};

/// Displays a native browser desktop notification if permission is granted,
/// or requests permission from the user if not yet decided.
pub fn show_browser_notification(title: &str, body: &str) -> Result<(), String> {
    if web_sys::window().is_none() {
        return Err("当前运行环境不支持 Window 对象".to_string());
    }

    let permission = Notification::permission();
    match permission {
        NotificationPermission::Granted => {
            let options = NotificationOptions::new();
            options.set_body(body);
            Notification::new_with_options(title, &options)
                .map_err(|e| format!("创建系统通知失败: {e:?}"))?;
            Ok(())
        }
        NotificationPermission::Denied => Err(
            "浏览器桌面通知权限已被禁用。请在浏览器地址栏左侧网站设置中开启「通知」权限。"
                .to_string(),
        ),
        _ => {
            let _ = Notification::request_permission();
            Err("已向浏览器申请桌面通知权限，请在弹出的系统提示中点击「允许」后再试。".to_string())
        }
    }
}
