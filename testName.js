const res = await fetch("https://www.youtube.com/watch?v=NjpmTL-ZM8E");
const html = await res.text();

function getYouTubeVideoTitle(htmlString) {
    const metaTitleStartTag = '<meta name="title" content="';
    const metaTitleEndTag = '">';

    const startIndex = htmlString.indexOf(metaTitleStartTag);

    // If the meta title tag is not found, return null or an empty string
    if (startIndex === -1) {
        return null;
    }

    // Calculate the starting index of the actual title content
    const contentStartIndex = startIndex + metaTitleStartTag.length;

    // Find the end index of the title content (the closing double quote)
    const endIndex = htmlString.indexOf(metaTitleEndTag, contentStartIndex);

    // If the closing quote is not found (which shouldn't happen with valid HTML), return null
    if (endIndex === -1) {
        return null;
    }

    // Extract the substring between the start of the content and the closing quote
    return htmlString.substring(contentStartIndex, endIndex);
}

console.log(getYouTubeVideoTitle(html));