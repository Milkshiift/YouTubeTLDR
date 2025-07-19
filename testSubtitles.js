function decodeXml(xml) {
    return xml.replace(/&/g, '&')
        .replace(/</g, '<')
        .replace(/>/g, '>')
        .replace(/"/g, '"')
        .replace(/'/g, "'");
}

async function getYoutubeTranscript(videoId, language = "en") {
    const videoUrl = `https://www.youtube.com/watch?v=${videoId}`;

    const html = await fetch(videoUrl).then(res => res.text());
    const apiKeyMatch = html.match(/"INNERTUBE_API_KEY":"([^"]+)"/);
    if (!apiKeyMatch) throw new Error("INNERTUBE_API_KEY not found.");
    const apiKey = apiKeyMatch[1];

    const playerData = await fetch(`https://www.youtube.com/youtubei/v1/player?key=${apiKey}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
            context: {
                client: {
                    clientName: "ANDROID",
                    clientVersion: "20.10.38"
                }
            },
            videoId
        })
    }).then(res => res.json());

    const tracks = playerData?.captions?.playerCaptionsTracklistRenderer?.captionTracks;
    if (!tracks) throw new Error("No captions found.");
    const track = tracks.find(t => t.languageCode === language);
    if (!track) throw new Error(`No captions for language: ${language}`);

    const baseUrl = track.baseUrl.replace(/&fmt=\w+$/, "");

    const xml = await fetch(baseUrl).then(res => res.text());

    const transcript = [];
    const regex = /<text start="([^"]+)" dur="([^"]+)">(.+?)<\/text>/g;
    const matches = xml.matchAll(regex);

    for (const match of matches) {
        const start = parseFloat(match[1]);
        const duration = parseFloat(match[2]);
        const caption = decodeXml(match[3]);

        transcript.push({
            caption,
            startTime: start,
            endTime: start + duration
        });
    }

    return transcript;
}

console.log(await getYoutubeTranscript("X9BblS3qGaU", "en"));