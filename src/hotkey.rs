use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput,
    VIRTUAL_KEY,
};

pub fn send_keys(inputs: &[VIRTUAL_KEY]) {
    let keys = inputs
        .iter()
        .map(|i| (i, KEYBD_EVENT_FLAGS(0)))
        .chain(inputs.iter().rev().map(|i| (i, KEYEVENTF_KEYUP)));

    let mut inputs: Vec<INPUT> = Vec::with_capacity(inputs.len() * 2);

    for (vk_key, state) in keys {
        inputs.push(INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: *vk_key,
                    wScan: 0,
                    dwFlags: state,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        });
    }

    std::thread::spawn(move || unsafe {
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    });
}
