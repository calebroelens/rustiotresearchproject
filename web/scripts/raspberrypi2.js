let data = [];
let allowed = false;
let global_data = [];

let chart_options = {
    chart: {
        height: 380,
        width: "100%",
        type: "area",
        animations: {
            initialAnimation: {
                enabled: false
            }
        }
    },
    series: [
    ],
    xaxis: {
        type: 'datetime'
    },
    noData: {
        text: "No data",
    },
    dataLabels: {
        enabled: false,
    }
};

let init = async () => {
    console.log("DOMContent Loaded");
    let apply_btn = document.getElementById("apply-btn");
    apply_btn.disabled = true;
    set_button_listeners();
    await fill_date_filter();
    set_filter_listeners();
    let chart = new ApexCharts(document.getElementById("temp-chart"), chart_options);
    document.getElementById("temp-chart").hidden = true;
    chart.render();

    window.setInterval(
        async () => {
            if(global_data.length > 0){
                let day_selector = document.getElementById("sel-day");
                let month_selector = document.getElementById("sel-month");
                let year_selector = document.getElementById("sel-year");
                let live_allowed = !(day_selector.value === "day" || month_selector.value === "month" || year_selector.value === "year");
                if (live_allowed){
                    await update_data_by_date();
                }
            }
        }, 50000
    )

    window.setInterval(
        async () => {
            if (global_data.length > 0){
                document.getElementById("temp-chart").hidden = false;
                let entries_found = document.getElementById("entry-count");
                entries_found.innerHTML = "<strong>Plotting data</strong>";

                chart.updateSeries([
                    {
                        name: "Temperature",
                        data: global_data
                    }
                ]);
                entries_found.innerHTML = "";
            }
        }
        , 10000);

};

let set_filter_listeners = async () => {
    let day_selector = document.getElementById("sel-day");
    let month_selector = document.getElementById("sel-month");
    let year_selector = document.getElementById("sel-year");
    day_selector.addEventListener("change", on_change_selector_data);
    month_selector.addEventListener("change", on_change_selector_data);
    year_selector.addEventListener("change", on_change_selector_data);
}

let on_change_selector_data = async (event)  => {
    // On change
    let day_selector = document.getElementById("sel-day");
    let month_selector = document.getElementById("sel-month");
    let year_selector = document.getElementById("sel-year");
    let apply_btn = document.getElementById("apply-btn");
    apply_btn.disabled =
        day_selector.value === "day" || month_selector.value === "month" || year_selector.value === "year";
    allowed = !apply_btn.disabled;
}

let fill_date_filter = async () => {
    let dates = await fetch("device_data_vars/date").then(data => data.json());
    let day_selector = document.getElementById("sel-day");
    let month_selector = document.getElementById("sel-month");
    let year_selector = document.getElementById("sel-year");
    let months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November","December"]
    for(let item of dates){
        let date = new Date(item);
        day_selector.innerHTML += `<option value="${date.getDate()}">${date.getDate()}</option>`;
        if (!month_selector.innerHTML.includes(`<option value="${date.getMonth()+1}">${months[date.getMonth()]}</option>`)){
            month_selector.innerHTML += `<option value="${date.getMonth()+1}">${months[date.getMonth()]}</option>`
        }
        if(!year_selector.innerHTML.includes(`<option value="${date.getFullYear()}">${date.getFullYear()}</option>`)){
            year_selector.innerHTML += `<option value="${date.getFullYear()}">${date.getFullYear()}</option>`;
        }
    }
}

let update_data_by_date = async () => {
    let update_btn = document.getElementById("apply-btn");
    let entries_found = document.getElementById("entry-count");
    entries_found.innerHTML = "<strong>Loading...</strong>";
    update_btn.disabled = true;
    let day = document.getElementById("sel-day").value;
    let month = document.getElementById("sel-month").value;
    let year = document.getElementById("sel-year").value;
    let date = new Date();
    date.setDate(day);
    date.setMonth(month-1);
    date.setFullYear(year);
    let fetch_data = await fetch(`device_data/temperature/${year}/${month}/${day}`).then(data => data.json());
    console.log(fetch_data);
    entries_found.innerHTML = `${fetch_data.length} found. Structuring data...`;
    update_btn.disabled = false;
    // Update the chart
    let new_data = [];
    let count = 0;
    for(let item of fetch_data){
        if(item["value"] > 2000 || item["value"] === 1023){
            continue;
        }
        entries_found.innerHTML = `${fetch_data.length} found. Progress: ${count+1}/${fetch_data.length-1}`;
        new_data.push(
            {
                x:  new Date(Date.parse(item["date_time"])).getTime(),
                y: item["value"]
            }
        );
        count++;
    }
    console.log(new_data);
    global_data = new_data;
    entries_found.innerHTML = `${fetch_data.length} found. Plotting...`;

}

let handle_new_ping =  (data) =>{
    // Handle the response
    let device_state = data["Temperature"] === 200;
    // Update the HTML
    let element = document.getElementById("service-status-color");
    let element2 = document.getElementById("service-status-circle");
    if(device_state){
        element.innerHTML = "Up";
        element.className = "";
        element.className = "text-green";
        element2.innerHTML = "<span class=\"status-indicator status-green status-indicator-animated\">\n" +
            "                                     <span class=\"status-indicator-circle\"></span>\n" +
            "                                    <span class=\"status-indicator-circle\"></span>\n" +
            "                                    <span class=\"status-indicator-circle\"></span>\n" +
            "                                 </span>";
    }
    else
    {
        element.innerHTML = "Down";
        element.className = "";
        element.className = "text-red";
        element2.innerHTML = "<span class=\"status-indicator status-red status-indicator-animated\">\n" +
            "                                    <span class=\"status-indicator-circle\"></span>\n" +
            "                                     <span class=\"status-indicator-circle\"></span>\n" +
            "                                    <span class=\"status-indicator-circle\"></span>\n" +
            "                                </span>";
    }
}
let check_state_event = async () => {
    // Check the current state:
    let state = await fetch("ping_all").then(response => response.json()).then(data => handle_new_ping(data));
}

let set_button_listeners = () => {
    let check_btn = document.getElementById("check-state");
    check_btn.addEventListener("click", check_state_event);
    let update_btn = document.getElementById("apply-btn");
    update_btn.addEventListener("click", update_data_by_date);
};

document.addEventListener("DOMContentLoaded", init);