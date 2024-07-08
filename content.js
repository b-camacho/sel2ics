browser.runtime.onMessage.addListener((message) => {
  if (message.command === "displayIcal") {
    displayIcal(message.icalContent);
  } else if (message.command === "displayError") {
    displayError(message.error);
  }
});

function displayIcal(icalContent) {
  const selection = window.getSelection();
  const range = selection.getRangeAt(0);
  const rect = range.getBoundingClientRect();

  const modal = document.createElement('div');
  modal.style.cssText = `
    position: absolute;
    background-color: #2C2C2C;
    color: #FFFAD1;
    padding: 15px;
    border-radius: 8px;
    box-shadow: 0 0 10px rgba(0,0,0,0.5);
    z-index: 10000;
    max-width: 300px;
    font-family: Arial, sans-serif;
    font-size: 14px;
  `;

  const closeButton = document.createElement('button');
  closeButton.textContent = '×';
  closeButton.style.cssText = `
    position: absolute;
    top: 5px;
    right: 5px;
    background-color: transparent;
    color: #FFFAD1;
    border: none;
    font-size: 18px;
    cursor: pointer;
    padding: 0;
    line-height: 1;
  `;
  closeButton.onclick = () => document.body.removeChild(modal);

  const addToCalendarLink = document.createElement('a');
  addToCalendarLink.textContent = 'Add to Calendar';
  addToCalendarLink.style.cssText = `
    display: inline-block;
    background-color: #4A4A4A;
    color: #FFFAD1;
    text-decoration: none;
    padding: 8px 12px;
    border-radius: 4px;
    cursor: pointer;
    transition: background-color 0.3s;
  `;
  addToCalendarLink.onmouseover = () => addToCalendarLink.style.backgroundColor = '#5A5A5A';
  addToCalendarLink.onmouseout = () => addToCalendarLink.style.backgroundColor = '#4A4A4A';
  addToCalendarLink.href = generateCalendarLink(icalContent);
  addToCalendarLink.setAttribute('download', 'event.ics');

  modal.appendChild(closeButton);
  modal.appendChild(addToCalendarLink);
  document.body.appendChild(modal);

  // Position the modal
  const modalRect = modal.getBoundingClientRect();
  let left = rect.right + window.pageXOffset;
  let top = rect.top + window.pageYOffset;

  // Adjust position if it would appear off-screen
  if (left + modalRect.width > window.innerWidth) {
    left = rect.left - modalRect.width + window.pageXOffset;
  }
  if (top + modalRect.height > window.innerHeight) {
    top = window.innerHeight - modalRect.height + window.pageYOffset;
  }

  modal.style.left = `${left}px`;
  modal.style.top = `${top}px`;
}

function generateCalendarLink(icalContent) {

  const blob = new Blob([icalContent], {type: 'text/calendar;charset=utf-8'});
  const url = URL.createObjectURL(blob);

  return url;
}


function displayError(error) {
  alert(`Error: ${error}`);
}
