const submitBut = document.getElementById("submit");
const youtubeUrlInput = document.getElementById("youtube-url");

const loaderEl = document.getElementById("loader");

const summaryEl = document.getElementById("summary-output");

const baseURL = `${location.protocol}//${location.hostname}${location.port ? ':' + location.port : ''}`;

submitBut.addEventListener("click", async () => {
    loaderEl.classList.remove("hidden");

    try {
        const url = youtubeUrlInput.value;

        const res = await fetch(baseURL+"/api/summarize", {
            method: "POST",
            body: JSON.stringify({
                url: url
            }),
            headers: {
                "Content-type": "application/json; charset=UTF-8"
            }
        });
        const json = await res.json();

        if (res.status !== 200) {
            if (json["error"]) {
                throw json["error"];
            }
            throw "Server responded with status code: "+res.status;
        }

        const summary = json["summary"];
        const subtitles = json["subtitles"];

        summaryEl.mdContent = summary;
        loaderEl.classList.add("hidden");
    } catch (e) {
        loaderEl.classList.add("error");
        summaryEl.mdContent = "";
        loaderEl.innerText = e;
    }
});