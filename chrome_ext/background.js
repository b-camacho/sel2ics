chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: "calInvite",
    title: "Cal Invite",
    contexts: ["selection"]
  });
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "calInvite") {
    console.log('clicked')
    const selectedText = info.selectionText;
    fetch('https://cal.chmod.site/cal', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        text: selectedText,
        tz: Intl.DateTimeFormat().resolvedOptions().timeZone
      })
    })
    .then(response => response.text())
    .then(icalData => {
      const blob = new Blob([icalData], { type: 'text/calendar' });
      // Convert blob to base64
      const reader = new FileReader();
      reader.readAsDataURL(blob);
      reader.onloadend = function() {
        const base64data = reader.result;
        chrome.downloads.download({
          url: base64data,
          filename: 'sel2ical-event.ics',
          conflictAction: 'overwrite',
          saveAs: false
        });
      }
    })
    .catch(error => console.error('Error:', error));
  }
});
