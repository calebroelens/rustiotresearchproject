let init = () => {
    let button = document.getElementById("action-buzzer");
    button.addEventListener("click", button_action_buzzer);
    let button2 = document.getElementById("action-led");
    button2.addEventListener("click", button_action_led);
};
let button_action_buzzer = async () => {
    // Execute button
    let click = await fetch("/device_actions/airquality").then(response =>  response.text()).then(data => console.log(data));
}
let button_action_led = async () => {
    let click = await fetch("/device_actions/temperature").then(response =>  response.text()).then(data => console.log(data));
}

document.addEventListener("DOMContentLoaded", init);