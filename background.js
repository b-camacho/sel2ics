browser.contextMenus.create({
  id: "generate-ical",
  title: "Generate iCal",
  contexts: ["selection"]
});

browser.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "generate-ical") {
    generateIcal(info.selectionText, tab.id);
  }
});

async function generateIcal(text, tabId) {
  const apiKey = ENVREPLACE[ANTHROPIC_API_KEY];
  const apiUrl = 'https://api.anthropic.com/v1/messages';

  const systemPrompt = `The following text includes a date and place. Produce a .ics (apple calendar) file based on this text. If you need to think about this more, put your steps between <thinking></thinking> tags. Put your final answer in <answer></answer> tags. Here is an example .ics file:
<example>
BEGIN:VCALENDAR
PRODID:-//xyz Corp//NONSGML PDA Calendar Version 1.0//EN
VERSION:2.0
BEGIN:VEVENT
DTSTAMP:19960704T120000Z
UID:uid1@example.com
ORGANIZER:mailto:jsmith@example.com
DTSTART:19960918T143000Z
DTEND:19960920T220000Z
STATUS:CONFIRMED
CATEGORIES:CONFERENCE
SUMMARY:Networld+Interop Conference
DESCRIPTION:Networld+Interop Conference
  and Exhibit\\nAtlanta World Congress Center\\n
 Atlanta\\, Georgia
END:VEVENT
END:VCALENDAR
</example>

Assume ET if timezone not mentioned.

If the text uses relative time (eg. "tomorrow"), calculate it assuming it is 6:21 pm on 2024-07-08. 
`;

  try {
    const response = await fetch(apiUrl, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-API-Key': apiKey,
        'anthropic-version': '2023-06-01'
      },
      body: JSON.stringify({
        model: "claude-3-haiku-20240307",
        max_tokens: 1000,
        system: systemPrompt,
        messages: [
          { role: "user", content: text }
        ]
      })
    });

    if (!response.ok) {
        msg = await response.text();
        throw new Error(`HTTP error! status: ${response.status}, message: ${msg}`);
    }

    const data = await response.json();
    const assistantMessage = data.content[0].text;

    // Extract the iCal content from the response
    const icalMatch = assistantMessage.match(/<answer>([\s\S]*?)<\/answer>/);
    const icalContent = icalMatch ? icalMatch[1].trim() : null;

    if (icalContent) {
      browser.tabs.sendMessage(tabId, {
        command: "displayIcal",
        icalContent: icalContent
      });
    } else {
      browser.tabs.sendMessage(tabId, {
        command: "displayError",
        error: "Failed to generate iCal content"
      });
    }
  } catch (error) {
    console.error('Error:', error);
    browser.tabs.sendMessage(tabId, {
      command: "displayError",
      error: error.message
    });
  }
}
