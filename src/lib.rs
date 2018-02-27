#[macro_use]
extern crate serde_json;
use serde_json::Value;

extern crate encoding;
use encoding::{Encoding, DecoderTrap};
use encoding::all::GB18030;

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate cqpsdk;

use std::ffi::CString;
use std::ffi::CStr;

use std::sync::Mutex;
use std::thread;
use std::io::prelude::*;
use std::net::{TcpStream, TcpListener, Shutdown};

use std::os::raw::c_char;

static mut CQP_CLIENT: cqpsdk::Client = cqpsdk::Client::new("me.robirt.rust.welcome");
lazy_static! {
    static ref TCP_CLIENT: Mutex<TcpStream> = Mutex::new(TcpStream::connect("127.0.0.1:7008").unwrap());
}

#[export_name="AppInfo"]
pub extern "stdcall" fn app_info() -> *const c_char {
    unsafe{ CString::new(CQP_CLIENT.app_info()).unwrap().into_raw() }
}

#[export_name="Initialize"]
pub extern "stdcall" fn initialize(auth_code: i32) -> i32 {
    unsafe { CQP_CLIENT.initialize(auth_code) };
    return 0;
}

// Type=1001 酷Q启动
// 无论本应用是否被启用，本函数都会在酷Q启动后执行一次，请在这里执行应用初始化代码。
// 如非必要，不建议在这里加载窗口。（可以添加菜单，让用户手动打开窗口）
#[export_name="CQPStartupHandler"]
pub extern "stdcall" fn cqp_startup_handler()->i32{
    return 0;
}

// Type=1002 酷Q退出
// 无论本应用是否被启用，本函数都会在酷Q退出前执行一次，请在这里执行插件关闭代码。
// 本函数调用完毕后，酷Q将很快关闭，请不要再通过线程等方式执行其他代码。
#[export_name="CQPExitHandler"]
pub extern "stdcall" fn cqp_exit_handler()->i32{
    return 0;
}

// Type=1003 应用已被启用
// 当应用被启用后，将收到此事件。
// 如果酷Q载入时应用已被启用，则在_eventStartup(Type=1001,酷Q启动)被调用后，本函数也将被调用一次。
// 如非必要，不建议在这里加载窗口。（可以添加菜单，让用户手动打开窗口）
#[export_name="EnableHandler"]
pub extern "stdcall" fn cqp_enable_handler()->i32{
    thread::spawn(||{
        match TcpListener::bind("127.0.0.1:7000"){
            Ok(listener) =>{
                unsafe{ CQP_CLIENT.add_log(cqpsdk::LogLevel::Debug,"rust json rpc","listening started, ready to accept") };
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            // CQP_CLIENT.add_log(cqpsdk::LogLevel::Info,"rust json rpc","收到接入");
                            thread::spawn(move|| {
                                handle_client(stream);
                            });
                        }
                        Err(e) => {
                            let error_msg = format!("{:?}", e);
                            unsafe{ CQP_CLIENT.add_log(cqpsdk::LogLevel::Error,"rust json rpc",error_msg.as_str()) };
                        }
                    }
                }
                drop(listener);
            }
            Err(e) => {
                let error_msg = format!("{:?}", e);
                unsafe{ CQP_CLIENT.add_log(cqpsdk::LogLevel::Error,"rust json rpc",error_msg.as_str()) };
            }
        }
    });
    return 0;
}

// Type=1004 应用将被停用
// 当应用被停用前，将收到此事件。
// 如果酷Q载入时应用已被停用，则本函数*不会*被调用。
// 无论本应用是否被启用，酷Q关闭前本函数都*不会*被调用。
#[export_name="DisableHandler"]
pub extern "stdcall" fn cqp_disable_handler()->i32{
    return 0;
}

// Type=21 私聊消息
// subType:11:来自好友 1:来自在线状态 2:来自群 3:来自讨论组
#[export_name="PrivateMessageHandler"]
pub extern "stdcall" fn private_message_handler(sub_type: i32, send_time: i32, qq_num: i64, msg: *const c_char, font: i32) -> i32 {
    let msg = unsafe{
        GB18030_C_CHAR_PRT_TO_UTF8_STR!(msg)
    };

    let notification = json!({"method":"PrivateMessage","params":{"subtype":sub_type,"sendtime":send_time,"qqnum":qq_num,"message":msg,"font":font}});
    send_notification(notification);
    return cqpsdk::EVENT_IGNORE;
}

// Type=2 群消息  subType固定为1
// fromQQ == 80000000 && strlen(fromAnonymous)>0 为 匿名消息
#[export_name="GroupMessageHandler"]
pub extern "stdcall" fn group_message_handler(sub_type: i32, send_time: i32, group_num: i64, qq_num: i64, anonymous_name: *const c_char, msg: *const c_char, font: i32) -> i32 {
    let msg = unsafe{
        GB18030_C_CHAR_PRT_TO_UTF8_STR!(msg)
    };
    let anonymous_name = unsafe{
        GB18030_C_CHAR_PRT_TO_UTF8_STR!(anonymous_name)
    };
    let notification = json!({"method":"GroupMessage","params":{"subtype":sub_type,"sendtime":send_time,"groupnum":group_num,"qqnum":qq_num,"anonymousname":anonymous_name,"message":msg,"font":font}});
    send_notification(notification);
    return cqpsdk::EVENT_IGNORE;
}

// Type=102 群事件-群成员减少
// subType 1/群员离开 2/群员被踢 3/自己(即登录号)被踢
// fromQQ, 操作者QQ(仅子类型为2、3时存在)
#[export_name="GroupMemberLeaveHandler"]
pub extern "stdcall" fn group_member_leave_handler(sub_type: i32, send_time: i32, group_num: i64, opqq_num: i64, qq_num: i64) -> i32 {
    let notification = json!({"method":"GroupMemberLeave","params":{"subtype":sub_type,"sendtime":send_time,"groupnum":group_num,"opqqnum":opqq_num,"qqnum":qq_num}});
    send_notification(notification);
    return cqpsdk::EVENT_IGNORE;
}

// Type=103 群事件-群成员增加
// subType 1/管理员已同意 2/管理员邀请
#[export_name="GroupMemberJoinHandler"]
pub extern "stdcall" fn group_member_join_handler(sub_type: i32, send_time: i32, group_num: i64, opqq_num: i64, qq_num: i64) -> i32 {
    let notification = json!({"method":"GroupMemberJoin","params":{"subtype":sub_type,"sendtime":send_time,"groupnum":group_num,"opqqnum":opqq_num,"qqnum":qq_num}});
    send_notification(notification);
    return cqpsdk::EVENT_IGNORE;
}

// Type=301 请求-好友添加
#[export_name="RequestAddFriendHandler"]
pub extern "stdcall" fn request_add_friend_handler(sub_type: i32, send_time: i32, from_qq: i64, msg: *const c_char, response_flag: *const c_char) -> i32 {
    let msg = unsafe{
        GB18030_C_CHAR_PRT_TO_UTF8_STR!(msg)
    };
    let response_flag = unsafe{
        GB18030_C_CHAR_PRT_TO_UTF8_STR!(response_flag)
    };
    let notification = json!({"method":"RequestAddFriend","params":{"subtype":sub_type,"sendtime":send_time,"fromqq":from_qq,"msg":msg,"response_flag":response_flag}});
    send_notification(notification);
    return cqpsdk::EVENT_IGNORE;
}

// Type=302 请求-群添加
#[export_name="RequestAddGroupHandler"]
pub extern "stdcall" fn request_add_group_handler(sub_type: i32, send_time: i32, group_num: i64, from_qq: i64, msg: *const c_char, response_flag: *const c_char) -> i32 {
    let msg = unsafe{
        GB18030_C_CHAR_PRT_TO_UTF8_STR!(msg)
    };
    let response_flag = unsafe{
        GB18030_C_CHAR_PRT_TO_UTF8_STR!(response_flag)
    };
    let notification = json!({"method":"RequestAddGroup","params":{"subtype":sub_type,"sendtime":send_time,"groupnum":group_num,"fromqq":from_qq,"msg":msg,"response_flag":response_flag}});
    send_notification(notification);
    return cqpsdk::EVENT_IGNORE;
}

// Type=4 讨论组消息处理
#[export_name="DiscussMessageHandler"]
pub extern "stdcall" fn discuss_message_handler(sub_type: i32, send_time: i32, from_discuss: i64, qq_num: i64, msg: *const c_char, font: i32) -> i32 {
    let msg = unsafe{
        GB18030_C_CHAR_PRT_TO_UTF8_STR!(msg);
    };
    let notification = json!({"method":"DiscussMessage","params":{"subtype":sub_type,"sendtime":send_time,"fromdiscuss":from_discuss,"fromqq":qq_num,"msg":msg,"font":font}});
    send_notification(notification);
    return cqpsdk::EVENT_IGNORE;
}

//
// ========== 分割线 ==========
//

fn send_notification(notification: serde_json::Value){
    let notification = format!("{}\n",notification);
    match TCP_CLIENT.lock(){
        Ok(mut client)=>{
            match client.write_all(notification.as_bytes()){
                Ok(_)=>{
                    // CQP_CLIENT.add_log(cqpsdk::CQLOG_INFO,"rust json rpc",&notification);
                }
                Err(e)=>{
                    let error_msg = format!("{:?}", e);
                    unsafe{ CQP_CLIENT.add_log(cqpsdk::LogLevel::Error,"rust json rpc",&error_msg) };
                }
            }
        }
        Err(e)=>{
            let error_msg = format!("{:?}", e);
            unsafe{ CQP_CLIENT.add_log(cqpsdk::LogLevel::Error,"rust json rpc",&error_msg) };
        }
    }
}

fn handle_client(mut stream :TcpStream){
    // unsafe{
    //     CQP_CLIENT.add_log(cqpsdk::CQLOG_INFO,"rust json rpc", "进入handle_client...");
    // };
    let mut request = String::new();
    let result = stream.read_to_string(&mut request);
    match result {
        Ok(_) => {
            unsafe{ CQP_CLIENT.add_log(cqpsdk::LogLevel::Debug,"rust json rpc",&request) };
            let json_value: Value = serde_json::from_str(&request).unwrap();
            let notification = json_value.as_object().unwrap();
            let method = notification.get("method").unwrap().as_str().unwrap();
            // CQP_CLIENT.add_log(cqpsdk::LogLevel::Debug,"rust json rpc", method)
            let params = notification.get("params").unwrap().as_object().unwrap();
            match method{
                "SendPrivateMessage" => {
                    let message = params.get("message").unwrap().as_str().unwrap();
                    let qqnum = params.get("qqnum").unwrap().as_i64().unwrap();
                    unsafe{ CQP_CLIENT.send_private_message(qqnum, message) };
                }
                "SendGroupMessage" => {
                    let message = params.get("message").unwrap().as_str().unwrap();
                    let groupnum = params.get("groupnum").unwrap().as_i64().unwrap();
                    unsafe{ CQP_CLIENT.send_group_msg(groupnum, message) };
                }
                "SendDiscussionMessage" => {
                    let message = params.get("message").unwrap().as_str().unwrap();
                    let discussion_num = params.get("discussionnum").unwrap().as_i64().unwrap();
                    unsafe{ CQP_CLIENT.send_discussion_msg(discussion_num, message) };
                }
                "GetToken" => {
                    let csrf_token = unsafe{ CQP_CLIENT.get_csrf_token() };
                    let login_qq = unsafe{ CQP_CLIENT.get_login_qq() };
                    let cookies = unsafe{ CQP_CLIENT.get_cookies() };
                    let cookies = cookies.to_owned();
                    let notification = json!({"method":"Token","params":{"token":csrf_token,"cookies":cookies,"loginqq":login_qq}});
                    send_notification(notification);
                }
                "FriendAdd" => {
                    let response_flag = params.get("responseFlag").unwrap().as_str().unwrap();
                    let accept = params.get("accept").unwrap().as_i64().unwrap() as i32;
                    let memo = params.get("memo").unwrap().as_str().unwrap();
                    unsafe{ CQP_CLIENT.set_friend_add_request(response_flag, accept, memo) };
                }
                "GroupAdd" => {
                    let response_flag = params.get("responseFlag").unwrap().as_str().unwrap();
                    let accept = params.get("accept").unwrap().as_i64().unwrap() as i32;
                    let sub_type = params.get("subType").unwrap().as_i64().unwrap() as i32;
                    let reason = params.get("reason").unwrap().as_str().unwrap();
                    unsafe{ CQP_CLIENT.set_group_add_request(response_flag, sub_type, accept, reason) };
                }
                "GroupLeave" => {
                    let groupnum = params.get("groupnum").unwrap().as_i64().unwrap();
                    let qqnum = params.get("qqnum").unwrap().as_i64().unwrap();
                    unsafe{ CQP_CLIENT.set_group_leave(groupnum, qqnum, 0) };
                }
                "GroupBan" => {
                    let groupnum = params.get("groupnum").unwrap().as_i64().unwrap();
                    let qqnum = params.get("qqnum").unwrap().as_i64().unwrap();
                    let seconds = params.get("seconds").unwrap().as_i64().unwrap();
                    unsafe{ CQP_CLIENT.set_group_ban(groupnum, qqnum, seconds) };
                }
                // "GetGroupMemberInfo"=>{//Auth=130 //getGroupMemberInfoV2
                //     let groupnum = params.get("groupnum").unwrap().as_i64().unwrap();
                //     let qqnum = params.get("qqnum").unwrap().as_i64().unwrap();
                //     unsafe{
                //         cqpapi::CQ_getGroupMemberInfoV2(CQP_CLIENT.auth_code, groupnum, qqnum, 0);
                //     };
                //     let notification = json!({"method":"GroupMemberInfo","params":{"token":csrf_token,"cookies":cookies,"loginqq":login_qq}});
                //     send_notification(notification);
                // }
                _ =>{
                    unsafe{ CQP_CLIENT.add_log(cqpsdk::LogLevel::Error,"rust json rpc default",&request) };
                }
            }
        }
        Err(e)=>{
            let error_msg = format!("{:?}", e);
            unsafe{ CQP_CLIENT.add_log(cqpsdk::LogLevel::Error,"rust json rpc error", &error_msg) };
        }
    }
    let _ = stream.shutdown(Shutdown::Both);
}