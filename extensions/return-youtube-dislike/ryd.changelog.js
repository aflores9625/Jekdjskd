/******/ (() => { // webpackBootstrap
/******/ 	"use strict";
var __webpack_exports__ = {};

;// CONCATENATED MODULE: ./Extensions/combined/src/config.js
const PROD_API_URL = "https://returnyoutubedislikeapi.com";
const DEV_API_URL = PROD_API_URL;

const runtime = typeof chrome !== "undefined" ? chrome.runtime : null;
const manifest = typeof runtime?.getManifest === "function" ? runtime.getManifest() : null;
const isDevelopment = !manifest || !("update_url" in manifest);

const extensionChangelogUrl =
  runtime && typeof runtime.getURL === "function"
    ? runtime.getURL("changelog/4/changelog_4.0.html")
    : "https://returnyoutubedislike.com/changelog/4/changelog_4.0.html";

const config = {
  apiUrl: isDevelopment ? DEV_API_URL : PROD_API_URL,

  voteDisabledIconName: "icon_hold128.png",
  defaultIconName: "icon128.png",

  links: {
    website: "https://returnyoutubedislike.com",
    github: "https://github.com/Anarios/return-youtube-dislike",
    discord: "https://discord.gg/mYnESY4Md5",
    donate: "https://returnyoutubedislike.com/donate",
    faq: "https://returnyoutubedislike.com/faq",
    help: "https://returnyoutubedislike.com/help",
    changelog: extensionChangelogUrl,
  },

  defaultExtConfig: {
    disableVoteSubmission: false,
    disableLogging: true,
    coloredThumbs: false,
    coloredBar: false,
    colorTheme: "classic",
    numberDisplayFormat: "compactShort",
    numberDisplayReformatLikes: false,
    hidePremiumTeaser: false,
  },
};

function getApiUrl() {
  return config.apiUrl;
}

function config_getApiEndpoint(endpoint) {
  return `${config.apiUrl}${endpoint.startsWith("/") ? "" : "/"}${endpoint}`;
}

function getChangelogUrl() {
  return config.links?.changelog ?? extensionChangelogUrl;
}



;// CONCATENATED MODULE: ./Extensions/combined/src/buttons.js



function getNativeButton(buttonContainer) {
  return querySelector(extConfig.selectors.buttons.nativeButton, buttonContainer);
}

function isSegmentedButtonLayout() {
  return querySelector(extConfig.selectors.buttons.segmentedContainer, buttons_getButtons()) !== undefined;
}

function buttons_getButtons() {
  //---   If Watching Youtube Shorts:   ---//
  if (isShorts()) {
    let elements = isMobile()
      ? querySelectorAll(extConfig.selectors.buttons.shorts.mobile)
      : querySelectorAll(extConfig.selectors.buttons.shorts.desktop);

    for (let element of elements) {
      //YouTube Shorts can have multiple like/dislike buttons when scrolling through videos
      //However, only one of them should be visible (no matter how you zoom)
      if (isInViewport(element)) {
        return element;
      }
    }

    if (elements.length > 0) {
      return elements[0];
    }
  }
  //---   If Watching On Mobile:   ---//
  if (isMobile()) {
    return document.querySelector(extConfig.selectors.buttons.regular.mobile);
  }
  //---   If Menu Element Is Displayed:   ---//
  if (querySelector(extConfig.selectors.menuContainer)?.offsetParent === null) {
    return querySelector(extConfig.selectors.buttons.regular.desktopMenu);
    //---   If Menu Element Isn't Displayed:   ---//
  } else {
    return querySelector(extConfig.selectors.buttons.regular.desktopNoMenu);
  }
}

function buttons_getLikeButton() {
  return isSegmentedButtonLayout()
    ? querySelector(extConfig.selectors.buttons.likeButton.segmented) ??
        querySelector(extConfig.selectors.buttons.likeButton.segmentedGetButtons, buttons_getButtons())
    : querySelector(extConfig.selectors.buttons.likeButton.notSegmented, buttons_getButtons());
}

function buttons_getLikeTextContainer() {
  return querySelector(extConfig.selectors.likeTextContainer, buttons_getLikeButton());
}

function buttons_getDislikeButton() {
  if (isSegmentedButtonLayout()) {
    return (
      querySelector(extConfig.selectors.buttons.dislikeButton.segmented) ??
      querySelector(extConfig.selectors.buttons.dislikeButton.segmentedGetButtons, buttons_getButtons())
    );
  }

  const notSegmentedMatch = querySelector(extConfig.selectors.buttons.dislikeButton.notSegmented, buttons_getButtons());

  if (notSegmentedMatch != null) {
    return notSegmentedMatch;
  }

  if (isShorts()) {
    return querySelector(extConfig.selectors.buttons.dislikeButton.shortsFallback, buttons_getButtons());
  }

  return null;
}

function getTextContainerTemplate() {
  const likeButton = buttons_getLikeButton();
  const parentTemplate =
    querySelector(extConfig.selectors.likeTextContainerTemplateParent, likeButton) ??
    querySelector(extConfig.selectors.likeTextContainerTemplateParent);

  return querySelector(extConfig.selectors.likeTextContainerTemplate, likeButton) ?? parentTemplate?.parentNode;
}

function updateDislikeButtonShape(dislikeButton) {
  for (const className of extConfig.selectors.buttonClasses.iconButton) {
    dislikeButton.classList.remove(className);
  }

  for (const className of extConfig.selectors.buttonClasses.iconLeading) {
    dislikeButton.classList.add(className);
  }
}

// Removes text written directly into an element (i.e. non-empty direct text
// node children), leaving child elements untouched. YouTube's server-side
// "dirty fix" injects the dislike count straight into the icon button via
// `innerText`, which both shows a stray number and wipes the icon; this lets us
// strip that so our own text container is the single source of the count.
function removeStrayButtonText(element) {
  if (!element) return;
  for (const node of Array.from(element.childNodes)) {
    if (node.nodeType === Node.TEXT_NODE && node.textContent.trim() !== "") {
      element.removeChild(node);
    }
  }
}

function createDislikeTextContainer() {
  const template = getTextContainerTemplate();
  const dislikeButton = getNativeButton(buttons_getDislikeButton());
  if (!template || !dislikeButton) {
    return null;
  }
  const textNodeClone = template.cloneNode(true);
  const insertPreChild = dislikeButton;
  removeStrayButtonText(dislikeButton);
  insertPreChild.insertBefore(textNodeClone, null);
  updateDislikeButtonShape(dislikeButton);
  if (querySelector(extConfig.selectors.textContainerInner, textNodeClone) === undefined) {
    const span = document.createElement("span");
    span.setAttribute("role", "text");
    while (textNodeClone.firstChild) {
      textNodeClone.removeChild(textNodeClone.firstChild);
    }
    textNodeClone.appendChild(span);
  }
  textNodeClone.innerText = "";
  return textNodeClone;
}

function buttons_getDislikeTextContainer() {
  const dislikeView = buttons_getDislikeButton();
  if (!dislikeView) {
    return null;
  }
  let result;
  const nativeDislikeButton = getNativeButton(dislikeView);
  for (const selector of extConfig.selectors.dislikeTextContainer) {
    result = dislikeView.querySelector(selector);
    if (result !== null && result !== nativeDislikeButton) {
      break;
    }
    result = null;
  }
  if (result == null) {
    result = createDislikeTextContainer();
  }
  // If the count ends up in a dedicated child container, make sure no count
  // also got written directly into the button (e.g. by the server-side dirty
  // fix), which would render as a duplicate number beside our container.
  if (nativeDislikeButton && result !== nativeDislikeButton) {
    removeStrayButtonText(nativeDislikeButton);
  }
  return result;
}

function checkForSignInButton() {
  if (querySelector(extConfig.selectors.signInButton)) {
    return true;
  } else {
    return false;
  }
}



;// CONCATENATED MODULE: ./Extensions/combined/src/bar.js




function bar_createRateBar(likes, dislikes) {
  let rateBar = document.getElementById("ryd-bar-container");
  if (!isLikesDisabled()) {
    // sometimes rate bar is hidden
    if (rateBar && !isInViewport(rateBar)) {
      rateBar.remove();
      rateBar = null;
    }

    const widthPx = getElementWidth(getLikeButton()) + getElementWidth(getDislikeButton()) + (isRoundedDesign() ? 0 : 8);

    const widthPercent = likes + dislikes > 0 ? (likes / (likes + dislikes)) * 100 : 50;

    var likePercentage = parseFloat(widthPercent.toFixed(1));
    const dislikePercentage = (100 - likePercentage).toLocaleString();
    likePercentage = likePercentage.toLocaleString();

    if (extConfig.showTooltipPercentage) {
      var tooltipInnerHTML;
      switch (extConfig.tooltipPercentageMode) {
        case "dash_dislike":
          tooltipInnerHTML = `${likes.toLocaleString()}&nbsp;/&nbsp;${dislikes.toLocaleString()}&nbsp;&nbsp;-&nbsp;&nbsp;${dislikePercentage}%`;
          break;
        case "both":
          tooltipInnerHTML = `${likePercentage}%&nbsp;/&nbsp;${dislikePercentage}%`;
          break;
        case "only_like":
          tooltipInnerHTML = `${likePercentage}%`;
          break;
        case "only_dislike":
          tooltipInnerHTML = `${dislikePercentage}%`;
          break;
        default: // dash_like
          tooltipInnerHTML = `${likes.toLocaleString()}&nbsp;/&nbsp;${dislikes.toLocaleString()}&nbsp;&nbsp;-&nbsp;&nbsp;${likePercentage}%`;
      }
    } else {
      tooltipInnerHTML = `${likes.toLocaleString()}&nbsp;/&nbsp;${dislikes.toLocaleString()}`;
    }

    if (!isShorts()) {
      if (!rateBar && !isMobile()) {
        let colorLikeStyle = "";
        let colorDislikeStyle = "";
        if (extConfig.coloredBar) {
          colorLikeStyle = "; background-color: " + getColorFromTheme(true);
          colorDislikeStyle = "; background-color: " + getColorFromTheme(false);
        }
        let actions =
          isNewDesign() && getButtons() === querySelector(extConfig.selectors.rateBar.newDesignActions)
            ? getButtons()
            : querySelector(extConfig.selectors.rateBar.oldDesignActions);
        (actions || querySelector(extConfig.selectors.rateBar.mobileActionBar)).insertAdjacentHTML(
          "beforeend",
          `
              <div class="ryd-tooltip ryd-tooltip-${isNewDesign() ? "new" : "old"}-design" style="width: ${widthPx}px">
              <div class="ryd-tooltip-bar-container">
                <div
                    id="ryd-bar-container"
                    style="width: 100%; height: 2px;${colorDislikeStyle}"
                    >
                    <div
                      id="ryd-bar"
                      style="width: ${widthPercent}%; height: 100%${colorLikeStyle}"
                      ></div>
                </div>
              </div>
              <tp-yt-paper-tooltip position="top" id="ryd-dislike-tooltip" class="style-scope ytd-sentiment-bar-renderer" role="tooltip" tabindex="-1">
                <!--css-build:shady-->${tooltipInnerHTML}
              </tp-yt-paper-tooltip>
              </div>
      		`,
        );

        if (isNewDesign()) {
          // Add border between info and comments
          let descriptionAndActionsElement = querySelector(extConfig.selectors.rateBar.topRow);
          descriptionAndActionsElement.style.borderBottom = "1px solid var(--yt-spec-10-percent-layer)";
          descriptionAndActionsElement.style.paddingBottom = "10px";

          // Fix like/dislike ratio bar offset in new UI
          querySelector(extConfig.selectors.rateBar.actionsInner).style.width = "revert";
          if (isRoundedDesign()) {
            querySelector(extConfig.selectors.rateBar.actions).style.flexDirection = "row-reverse";
          }
        }
      } else {
        document.querySelector(`.ryd-tooltip`).style.width = widthPx + "px";
        document.getElementById("ryd-bar").style.width = widthPercent + "%";
        document.querySelector("#ryd-dislike-tooltip > #tooltip").innerHTML = tooltipInnerHTML;
        if (extConfig.coloredBar) {
          document.getElementById("ryd-bar-container").style.backgroundColor = getColorFromTheme(false);
          document.getElementById("ryd-bar").style.backgroundColor = getColorFromTheme(true);
        }
      }
    }
  } else {
    console.log("removing bar");
    if (rateBar) {
      rateBar.parentNode.removeChild(rateBar);
    }
  }
}



;// CONCATENATED MODULE: ./Extensions/combined/src/state.js




const LIKED_STATE = "LIKED_STATE";
const DISLIKED_STATE = "DISLIKED_STATE";
const NEUTRAL_STATE = "NEUTRAL_STATE";

const DEFAULT_SELECTORS = {
  dislikeTextContainer: [
    ".yt-spec-button-shape-next__button-text-content",
    ".ytSpecButtonShapeNextButtonTextContent",
    "#text",
    "yt-formatted-string",
    "span[role='text']",
  ],
  likeTextContainer: [
    ".yt-spec-button-shape-next__button-text-content",
    ".ytSpecButtonShapeNextButtonTextContent",
    "#text",
    "yt-formatted-string",
    "span[role='text']",
  ],
  likeTextContainerTemplate: [
    ".yt-spec-button-shape-next__button-text-content",
    ".ytSpecButtonShapeNextButtonTextContent",
    "button > div[class*='cbox']",
  ],
  likeTextContainerTemplateParent: [
    'div > span[role="text"]',
    'button > div.yt-spec-button-shape-next__button-text-content > span[role="text"]',
  ],
  textContainerInner: ["span[role='text']"],
  buttons: {
    shorts: {
      mobile: ["ytm-like-button-renderer"],
      desktop: ["reel-action-bar-view-model", "#like-button > ytd-like-button-renderer"],
    },
    regular: {
      mobile: [".slim-video-action-bar-actions"],
      desktopMenu: ["ytd-menu-renderer.ytd-watch-metadata > div"],
      desktopNoMenu: ["#top-level-buttons-computed"],
    },
    segmentedContainer: ["ytd-segmented-like-dislike-button-renderer"],
    nativeButton: ["button"],
    mobileText: [".button-renderer-text"],
    shortsToggleButton: ["tp-yt-paper-button#button"],
    smartimation: ["yt-smartimation"],
    likeButton: {
      segmented: ["#segmented-like-button"],
      segmentedGetButtons: [":first-child > :first-child"],
      notSegmented: ["like-button-view-model", ":first-child"],
    },
    dislikeButton: {
      segmented: ["#segmented-dislike-button"],
      segmentedGetButtons: [":first-child > :nth-child(2)"],
      notSegmented: ["dislike-button-view-model", ":nth-child(2)", "#dislike-button"],
      shortsFallback: ["#dislike-button"],
    },
  },
  buttonClasses: {
    iconButton: ["yt-spec-button-shape-next--icon-button", "ytSpecButtonShapeNextIconButton"],
    iconLeading: ["yt-spec-button-shape-next--icon-leading", "ytSpecButtonShapeNextIconLeading"],
  },
  activeButtonClasses: ["style-default-active"],
  likeCountButton: ["yt-formatted-string#text", "button"],
  videoLoaded: [
    "ytd-watch-grid[video-id='{videoId}']",
    "ytd-watch-flexy[video-id='{videoId}']",
    '#player[loading="false"]:not([hidden])',
  ],
  shortsLoaded: {
    containers: [".reel-video-in-sequence-new"],
    thumbnail: [".reel-video-in-sequence-thumbnail"],
    renderer: ["ytd-reel-video-renderer"],
    overlay: ["#experiment-overlay"],
  },
  rateBar: {
    newDesignActions: ["#top-level-buttons-computed"],
    oldDesignActions: ["#menu-container"],
    mobileActionBar: ["ytm-slim-video-action-bar-renderer"],
    topRow: ["#top-row"],
    actionsInner: ["#actions-inner"],
    actions: ["#actions"],
  },
  signInButton: ["a[href^='https://accounts.google.com/ServiceLogin']"],
  menuContainer: ["#menu-container"],
  roundedDesign: ["#segmented-like-button", "like-button-view-model"],
};

function cloneConfig(value) {
  if (value === undefined) return undefined;
  return JSON.parse(JSON.stringify(value));
}

function mergeConfig(defaultValue, apiValue) {
  if (apiValue === undefined || apiValue === null) {
    return cloneConfig(defaultValue);
  }

  if (Array.isArray(apiValue)) {
    return [...apiValue];
  }

  if (typeof apiValue !== "object" || Array.isArray(defaultValue)) {
    return apiValue;
  }

  const merged = cloneConfig(defaultValue ?? {});
  for (const [key, value] of Object.entries(apiValue)) {
    merged[key] = mergeConfig(defaultValue?.[key], value);
  }
  return merged;
}

let state_extConfig = {
  disableVoteSubmission: false,
  disableLogging: false,
  coloredThumbs: false,
  coloredBar: false,
  colorTheme: "classic",
  numberDisplayFormat: "compactShort",
  showTooltipPercentage: false,
  tooltipPercentageMode: "dash_like",
  numberDisplayReformatLikes: false,
  hidePremiumTeaser: false,
  selectors: cloneConfig(DEFAULT_SELECTORS),
};

let storedData = {
  likes: 0,
  dislikes: 0,
  previousState: NEUTRAL_STATE,
};

function state_isMobile() {
  return location.hostname == "m.youtube.com";
}

function state_isShorts() {
  return location.pathname.startsWith("/shorts");
}

function state_isNewDesign() {
  return document.getElementById("comment-teaser") !== null;
}

function state_isRoundedDesign() {
  return querySelector(state_extConfig.selectors.roundedDesign) !== null;
}

let shortsObserver = null;

if (state_isShorts() && !shortsObserver) {
  console.log("Initializing shorts mutation observer");
  shortsObserver = createObserver(
    {
      attributes: true,
    },
    (mutationList) => {
      mutationList.forEach((mutation) => {
        if (
          mutation.type === "attributes" &&
          mutation.target.nodeName === "TP-YT-PAPER-BUTTON" &&
          mutation.target.id === "button"
        ) {
          // console.log('Short thumb button status changed');
          if (mutation.target.getAttribute("aria-pressed") === "true") {
            mutation.target.style.color =
              mutation.target.parentElement.parentElement.id === "like-button"
                ? utils_getColorFromTheme(true)
                : utils_getColorFromTheme(false);
          } else {
            mutation.target.style.color = "unset";
          }
          return;
        }
        console.log("Unexpected mutation observer event: " + mutation.target + mutation.type);
      });
    },
  );
}

function state_isLikesDisabled() {
  // return true if the like button's text doesn't contain any number
  if (state_isMobile()) {
    const buttons = getButtons();
    const mobileText = buttons?.children?.[0]
      ? querySelector(state_extConfig.selectors.buttons.mobileText, buttons.children[0])
      : null;
    if (!mobileText) return true;
    return /^\D*$/.test(mobileText.innerText);
  }
  const likeText = getLikeTextContainer();
  if (!likeText) return true;
  return /^\D*$/.test(likeText.innerText);
}

function isVideoLiked() {
  const likeView = getLikeButton();
  if (!likeView) {
    return false;
  }
  const likeButton = querySelector(state_extConfig.selectors.buttons.nativeButton, likeView);
  if (state_isMobile()) {
    return likeButton?.getAttribute("aria-label") === "true";
  }
  return (
    state_extConfig.selectors.activeButtonClasses.some((className) => likeView.classList.contains(className)) ||
    likeButton?.getAttribute("aria-pressed") === "true"
  );
}

function isVideoDisliked() {
  const dislikeView = getDislikeButton();
  if (!dislikeView) {
    return false;
  }
  const dislikeButton = querySelector(state_extConfig.selectors.buttons.nativeButton, dislikeView);
  if (state_isMobile()) {
    return dislikeButton?.getAttribute("aria-label") === "true";
  }
  return (
    state_extConfig.selectors.activeButtonClasses.some((className) => dislikeView.classList.contains(className)) ||
    dislikeButton?.getAttribute("aria-pressed") === "true"
  );
}

function getState(storedData) {
  if (isVideoLiked()) {
    return { current: LIKED_STATE, previous: storedData.previousState };
  }
  if (isVideoDisliked()) {
    return { current: DISLIKED_STATE, previous: storedData.previousState };
  }
  return { current: NEUTRAL_STATE, previous: storedData.previousState };
}

//---   Sets The Likes And Dislikes Values   ---//
function setLikes(likesCount) {
  console.log(`SET likes ${likesCount}`);
  const container = getLikeTextContainer();
  if (container) {
    container.innerText = likesCount;
  }
}

function setDislikes(dislikesCount) {
  console.log(`SET dislikes ${dislikesCount}`);

  const _container = getDislikeTextContainer();
  _container?.removeAttribute("is-empty");

  let _dislikeText;
  if (!state_isLikesDisabled()) {
    if (state_isMobile()) {
      querySelector(state_extConfig.selectors.buttons.mobileText, getButtons().children[1]).innerText = dislikesCount;
      return;
    }
    _dislikeText = dislikesCount;
  } else {
    console.log("likes count disabled by creator");
    if (state_isMobile()) {
      querySelector(state_extConfig.selectors.buttons.mobileText, getButtons().children[1]).innerText =
        localize("TextLikesDisabled");
      return;
    }
    _dislikeText = localize("TextLikesDisabled");
  }

  if (_dislikeText != null && _container?.innerText !== _dislikeText) {
    _container.innerText = _dislikeText;
  }
}

function getLikeCountFromButton() {
  try {
    if (state_isShorts()) {
      //Youtube Shorts don't work with this query. It's not necessary; we can skip it and still see the results.
      //It should be possible to fix this function, but it's not critical to showing the dislike count.
      return false;
    }

    let likeButton = querySelector(state_extConfig.selectors.likeCountButton, getLikeButton());

    let likesStr = likeButton.getAttribute("aria-label").replace(/\D/g, "");
    return likesStr.length > 0 ? parseInt(likesStr) : false;
  } catch {
    return false;
  }
}

function processResponse(response, storedData) {
  const formattedDislike = numberFormat(response.dislikes);
  setDislikes(formattedDislike);
  if (state_extConfig.numberDisplayReformatLikes === true) {
    const nativeLikes = getLikeCountFromButton();
    if (nativeLikes !== false) {
      setLikes(numberFormat(nativeLikes));
    }
  }
  storedData.dislikes = parseInt(response.dislikes);
  storedData.likes = getLikeCountFromButton() || parseInt(response.likes);
  createRateBar(storedData.likes, storedData.dislikes);
  if (state_extConfig.coloredThumbs === true) {
    if (state_isShorts()) {
      // for shorts, leave deactivated buttons in default color
      let shortLikeButton = querySelector(state_extConfig.selectors.buttons.shortsToggleButton, getLikeButton());
      let shortDislikeButton = querySelector(state_extConfig.selectors.buttons.shortsToggleButton, getDislikeButton());
      if (shortLikeButton.getAttribute("aria-pressed") === "true") {
        shortLikeButton.style.color = getColorFromTheme(true);
      }
      if (shortDislikeButton.getAttribute("aria-pressed") === "true") {
        shortDislikeButton.style.color = getColorFromTheme(false);
      }
      shortsObserver.observe(shortLikeButton);
      shortsObserver.observe(shortDislikeButton);
    } else {
      getLikeButton().style.color = getColorFromTheme(true);
      getDislikeButton().style.color = getColorFromTheme(false);
    }
  }

  //Temporary disabling this - it breaks all places where getButtons()[1] is used
  // createStarRating(response.rating, isMobile());
}

// Tells the user if the API is down
function displayError(error) {
  const container = getDislikeTextContainer();
  if (container) {
    container.innerText = localize("textTempUnavailable");
  }
}

async function setState(storedData) {
  if (typeof window !== "undefined") {
    window.__rydSetStateCalls = (window.__rydSetStateCalls || 0) + 1;
  }
  storedData.previousState = isVideoDisliked() ? DISLIKED_STATE : isVideoLiked() ? LIKED_STATE : NEUTRAL_STATE;
  let statsSet = false;
  console.log("Video is loaded. Adding buttons...");

  let videoId = getVideoId(window.location.href);
  let likeCount = getLikeCountFromButton() || null;

  let response = await fetch(getApiEndpoint(`/votes?videoId=${videoId}&likeCount=${likeCount || ""}`), {
    method: "GET",
    headers: {
      Accept: "application/json",
    },
  })
    .then((response) => {
      if (!response.ok) displayError(response.error);
      return response;
    })
    .then((response) => response.json())
    .catch(displayError);
  console.log("response from api:");
  console.log(JSON.stringify(response));
  if (response !== undefined && !("traceId" in response) && !statsSet) {
    processResponse(response, storedData);
  }
}

async function setInitialState() {
  await setState(storedData);
}

async function initExtConfig() {
  initializeDisableVoteSubmission();
  initializeDisableLogging();
  initializeColoredThumbs();
  initializeColoredBar();
  initializeColorTheme();
  initializeNumberDisplayFormat();
  initializeTooltipPercentage();
  initializeTooltipPercentageMode();
  initializeNumberDisplayReformatLikes();
  initializeHidePremiumTeaser();
  await initializeSelectors();
}

async function initializeSelectors() {
  let result = await fetch(getApiEndpoint("/configs/selectors"), {
    method: "GET",
    headers: {
      Accept: "application/json",
    },
  })
    .then((response) => response.json())
    .catch((error) => {
      console.error("Error fetching selectors:", error);
    });
  state_extConfig.selectors = mergeConfig(DEFAULT_SELECTORS, result);
  if (!state_extConfig.selectors?.buttons || typeof state_extConfig.selectors.buttons !== "object") {
    state_extConfig.selectors = state_extConfig.selectors ?? cloneConfig(DEFAULT_SELECTORS);
    state_extConfig.selectors.buttons = cloneConfig(DEFAULT_SELECTORS.buttons);
  }
  console.log(result);
}

function initializeDisableVoteSubmission() {
  getBrowser().storage.sync.get(["disableVoteSubmission"], (res) => {
    if (res.disableVoteSubmission === undefined) {
      getBrowser().storage.sync.set({ disableVoteSubmission: false });
    } else {
      state_extConfig.disableVoteSubmission = res.disableVoteSubmission;
    }
  });
}

function initializeDisableLogging() {
  getBrowser().storage.sync.get(["disableLogging"], (res) => {
    if (res.disableLogging === undefined) {
      getBrowser().storage.sync.set({ disableLogging: true });
      state_extConfig.disableLogging = true;
    } else {
      state_extConfig.disableLogging = res.disableLogging;
    }
    // Initialize console methods based on logging config
    initializeLogging();
  });
}

function initializeColoredThumbs() {
  getBrowser().storage.sync.get(["coloredThumbs"], (res) => {
    if (res.coloredThumbs === undefined) {
      getBrowser().storage.sync.set({ coloredThumbs: false });
    } else {
      state_extConfig.coloredThumbs = res.coloredThumbs;
    }
  });
}

function initializeColoredBar() {
  getBrowser().storage.sync.get(["coloredBar"], (res) => {
    if (res.coloredBar === undefined) {
      getBrowser().storage.sync.set({ coloredBar: false });
    } else {
      state_extConfig.coloredBar = res.coloredBar;
    }
  });
}

function initializeColorTheme() {
  getBrowser().storage.sync.get(["colorTheme"], (res) => {
    if (res.colorTheme === undefined) {
      getBrowser().storage.sync.set({ colorTheme: false });
    } else {
      state_extConfig.colorTheme = res.colorTheme;
    }
  });
}

function initializeNumberDisplayFormat() {
  getBrowser().storage.sync.get(["numberDisplayFormat"], (res) => {
    if (res.numberDisplayFormat === undefined) {
      getBrowser().storage.sync.set({ numberDisplayFormat: "compactShort" });
    } else {
      state_extConfig.numberDisplayFormat = res.numberDisplayFormat;
    }
  });
}

function initializeTooltipPercentage() {
  getBrowser().storage.sync.get(["showTooltipPercentage"], (res) => {
    if (res.showTooltipPercentage === undefined) {
      getBrowser().storage.sync.set({ showTooltipPercentage: false });
    } else {
      state_extConfig.showTooltipPercentage = res.showTooltipPercentage;
    }
  });
}

function initializeTooltipPercentageMode() {
  getBrowser().storage.sync.get(["tooltipPercentageMode"], (res) => {
    if (res.tooltipPercentageMode === undefined) {
      getBrowser().storage.sync.set({ tooltipPercentageMode: "dash_like" });
    } else {
      state_extConfig.tooltipPercentageMode = res.tooltipPercentageMode;
    }
  });
}

function initializeNumberDisplayReformatLikes() {
  getBrowser().storage.sync.get(["numberDisplayReformatLikes"], (res) => {
    if (res.numberDisplayReformatLikes === undefined) {
      getBrowser().storage.sync.set({ numberDisplayReformatLikes: false });
    } else {
      state_extConfig.numberDisplayReformatLikes = res.numberDisplayReformatLikes;
    }
  });
}

function initializeHidePremiumTeaser() {
  getBrowser().storage.sync.get(["hidePremiumTeaser"], (res) => {
    if (res.hidePremiumTeaser === undefined) {
      getBrowser().storage.sync.set({ hidePremiumTeaser: false });
      state_extConfig.hidePremiumTeaser = false;
    } else {
      state_extConfig.hidePremiumTeaser = res.hidePremiumTeaser === true;
    }
  });
}



;// CONCATENATED MODULE: ./Extensions/combined/src/utils.js


const DEFAULT_SHORTS_LOADED_SELECTORS = {
  containers: [".reel-video-in-sequence-new"],
  thumbnail: [".reel-video-in-sequence-thumbnail"],
  renderer: ["ytd-reel-video-renderer"],
  overlay: ["#experiment-overlay"],
};

const DEFAULT_VIDEO_LOADED_SELECTORS = (/* unused pure expression or super */ null && ([
  "ytd-watch-grid[video-id='{videoId}']",
  "ytd-watch-flexy[video-id='{videoId}']",
  '#player[loading="false"]:not([hidden])',
]));

function utils_numberFormat(numberState) {
  return getNumberFormatter(extConfig.numberDisplayFormat).format(numberState);
}

function getNumberFormatter(optionSelect) {
  let userLocales;
  if (document.documentElement.lang) {
    userLocales = document.documentElement.lang;
  } else if (navigator.language) {
    userLocales = navigator.language;
  } else {
    try {
      userLocales = new URL(
        Array.from(document.querySelectorAll("head > link[rel='search']"))
          ?.find((n) => n?.getAttribute("href")?.includes("?locale="))
          ?.getAttribute("href"),
      )?.searchParams?.get("locale");
    } catch {
      console.log("Cannot find browser locale. Use en as default for number formatting.");
      userLocales = "en";
    }
  }

  let formatterNotation;
  let formatterCompactDisplay;
  switch (optionSelect) {
    case "compactLong":
      formatterNotation = "compact";
      formatterCompactDisplay = "long";
      break;
    case "standard":
      formatterNotation = "standard";
      formatterCompactDisplay = "short";
      break;
    case "compactShort":
    default:
      formatterNotation = "compact";
      formatterCompactDisplay = "short";
  }

  return Intl.NumberFormat(userLocales, {
    notation: formatterNotation,
    compactDisplay: formatterCompactDisplay,
  });
}

function utils_localize(localeString, substitutions) {
  try {
    if (typeof chrome !== "undefined" && chrome?.i18n?.getMessage) {
      const args = substitutions === undefined ? [localeString] : [localeString, substitutions];
      const message = chrome.i18n.getMessage(...args);
      if (message) {
        return message;
      }
    }
  } catch (error) {
    console.warn("Localization lookup failed for", localeString, error);
  }

  if (Array.isArray(substitutions)) {
    return substitutions.join(" ");
  }

  if (substitutions != null) {
    return `${substitutions}`;
  }

  return localeString;
}

function utils_getBrowser() {
  if (typeof chrome !== "undefined" && typeof chrome.runtime !== "undefined") {
    return chrome;
  } else if (typeof browser !== "undefined" && typeof browser.runtime !== "undefined") {
    return browser;
  } else {
    console.log("browser is not supported");
    return false;
  }
}

function utils_getVideoId(url) {
  const urlObject = new URL(url);
  const pathname = urlObject.pathname;
  if (pathname.startsWith("/clip")) {
    return (document.querySelector("meta[itemprop='videoId']") || document.querySelector("meta[itemprop='identifier']"))
      .content;
  } else {
    if (pathname.startsWith("/shorts")) {
      return pathname.slice(8);
    }
    return urlObject.searchParams.get("v");
  }
}

function utils_isInViewport(element) {
  const rect = element.getBoundingClientRect();
  const height = innerHeight || document.documentElement.clientHeight;
  const width = innerWidth || document.documentElement.clientWidth;
  return (
    // When short (channel) is ignored, the element (like/dislike AND short itself) is
    // hidden with a 0 DOMRect. In this case, consider it outside of Viewport
    !(rect.top == 0 && rect.left == 0 && rect.bottom == 0 && rect.right == 0) &&
    rect.top >= 0 &&
    rect.left >= 0 &&
    rect.bottom <= height &&
    rect.right <= width
  );
}

function isShortsLoaded(videoId) {
  if (!videoId) return false;

  const selectors = extConfig.selectors.shortsLoaded ?? DEFAULT_SHORTS_LOADED_SELECTORS;

  // Find all reel containers
  const reelContainers = utils_querySelectorAll(selectors.containers);

  for (const container of reelContainers) {
    // Check if this container's thumbnail matches our video ID
    const thumbnail = utils_querySelector(selectors.thumbnail, container);
    if (thumbnail) {
      const bgImage = thumbnail.style.backgroundImage;
      // YouTube thumbnail URLs contain the video ID in the format: /vi/VIDEO_ID/
      if ((bgImage && bgImage.includes(`/${videoId}/`)) || (!bgImage && utils_isInViewport(container))) {
        // Check if this container has the renderer with visible experiment-overlay
        const renderer = utils_querySelector(selectors.renderer, container);
        if (renderer) {
          const experimentOverlay = utils_querySelector(selectors.overlay, renderer);
          if (
            experimentOverlay &&
            !experimentOverlay.hidden &&
            window.getComputedStyle(experimentOverlay).display !== "none" &&
            experimentOverlay.hasChildNodes()
          ) {
            return true;
          }
        }
      }
    }
  }

  return false;
}

function isVideoLoaded() {
  const videoId = utils_getVideoId(window.location.href);

  // Check if this is a Shorts URL
  if (isShorts()) {
    return isShortsLoaded(videoId);
  }

  const videoLoadedSelectors = extConfig.selectors.videoLoaded ?? DEFAULT_VIDEO_LOADED_SELECTORS;

  // Regular video checks
  return utils_querySelector(videoLoadedSelectors.map((selector) => selector.replace("{videoId}", videoId))) !== undefined;
}

const originalConsole = {
  log: console.log.bind(console),
  debug: console.debug.bind(console),
  info: console.info.bind(console),
  warn: console.warn.bind(console),
  error: console.error.bind(console),
};

function utils_initializeLogging() {
  if (extConfig.disableLogging) {
    console.log = () => {};
    console.debug = () => {};
  } else {
    console.log = originalConsole.log;
    console.debug = originalConsole.debug;
  }
}

function utils_getColorFromTheme(voteIsLike) {
  let colorString;
  switch (state_extConfig.colorTheme) {
    case "accessible":
      if (voteIsLike === true) {
        colorString = "dodgerblue";
      } else {
        colorString = "gold";
      }
      break;
    case "neon":
      if (voteIsLike === true) {
        colorString = "aqua";
      } else {
        colorString = "magenta";
      }
      break;
    case "classic":
    default:
      if (voteIsLike === true) {
        colorString = "lime";
      } else {
        colorString = "red";
      }
  }
  return colorString;
}

function utils_getElementWidth(element) {
  if (!element) return 0;
  // getComputedStyle().width resolves to "auto"/"" (→ NaN) for inline
  // elements such as YouTube's newer <like-button-view-model> /
  // <dislike-button-view-model> wrappers. Fall back to the rendered box width
  // so the ratio bar still gets a sensible size.
  const styleWidth = parseFloat(window.getComputedStyle(element).width);
  if (Number.isFinite(styleWidth)) return styleWidth;
  const rect = element.getBoundingClientRect?.();
  return rect ? rect.width : 0;
}

function utils_querySelector(selectors, element) {
  let result;
  for (const selector of Array.isArray(selectors) ? selectors : [selectors]) {
    if (!selector) continue;
    result = (element ?? document).querySelector(selector);
    if (result !== null) {
      return result;
    }
  }
}

function utils_querySelectorAll(selectors) {
  let result;
  for (const selector of Array.isArray(selectors) ? selectors : [selectors]) {
    if (!selector) continue;
    result = document.querySelectorAll(selector);
    if (result.length !== 0) {
      return result;
    }
  }
  return result ?? document.querySelectorAll("__ryd-missing-selector__");
}

function createObserver(options, callback) {
  const observerWrapper = new Object();
  observerWrapper.options = options;
  observerWrapper.observer = new MutationObserver(callback);
  observerWrapper.observe = function (element) {
    this.observer.observe(element, this.options);
  };
  observerWrapper.disconnect = function () {
    this.observer.disconnect();
  };
  return observerWrapper;
}



;// CONCATENATED MODULE: ./Extensions/combined/src/changelog/index.js



const PATREON_JOIN_URL = "https://www.patreon.com/join/returnyoutubedislike/checkout?rid=8008649";
const SUPPORT_DOC_URL = config.links?.help ?? "https://returnyoutubedislike.com/help";
const COMMUNITY_URL = config.links?.discord ?? "https://discord.gg/mYnESY4Md5";

function initChangelogPage() {
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", setup);
  } else {
    setup();
  }
}

function setup() {
  applyLocaleMetadata();
  localizeHtmlPage();
  decorateScreenshotPlaceholders();
  bindActions();
}

function applyLocaleMetadata() {
  try {
    const browserLocale = chrome?.i18n?.getMessage?.("@@ui_locale");
    if (browserLocale) {
      document.documentElement.lang = browserLocale;
    }
  } catch (error) {
    console.debug("Unable to resolve UI locale", error);
  }
}

function localizeHtmlPage() {
  const elements = document.getElementsByTagName("html");
  for (let index = 0; index < elements.length; index += 1) {
    const element = elements[index];
    const original = element.innerHTML.toString();
    const localized = original.replace(/__MSG_(\w+)__/g, (match, key) => {
      return key ? utils_localize(key) : "";
    });

    if (localized !== original) {
      element.innerHTML = localized;
    }
  }
}

function decorateScreenshotPlaceholders() {
  document.querySelectorAll("[data-screenshot]").forEach((wrapper) => {
    const type = wrapper.getAttribute("data-screenshot");
    const labelKey = getPlaceholderLabelKey(type);
    if (!labelKey) return;

    const placeholder = wrapper.querySelector(".ryd-feature-card__placeholder");
    if (!placeholder) return;

    const label = utils_localize(labelKey);
    placeholder.setAttribute("role", "img");
    placeholder.setAttribute("aria-label", label);
    placeholder.title = label;
  });
}

function getPlaceholderLabelKey(type) {
  switch (type) {
    case "timeline":
      return "changelog_screenshot_label_timeline";
    case "map":
      return "changelog_screenshot_label_map";
    case "teaser":
      return "changelog_screenshot_label_teaser";
    default:
      return null;
  }
}

function bindActions() {
  const browser = utils_getBrowser();

  const upgradeButton = document.getElementById("ryd-changelog-upgrade");
  if (upgradeButton) {
    upgradeButton.addEventListener("click", (event) => {
      event.preventDefault();
      openExternal(PATREON_JOIN_URL, browser);
    });
  }

  const supportButton = document.getElementById("ryd-changelog-support");
  if (supportButton) {
    supportButton.addEventListener("click", (event) => {
      event.preventDefault();
      openExternal(SUPPORT_DOC_URL, browser);
    });
  }

  const contactButton = document.getElementById("ryd-changelog-contact");
  if (contactButton) {
    contactButton.addEventListener("click", (event) => {
      event.preventDefault();
      openExternal(COMMUNITY_URL, browser);
    });
  }
}

function openExternal(url, browser) {
  if (!url) return;

  try {
    if (browser && browser.tabs && typeof browser.tabs.create === "function") {
      browser.tabs.create({ url });
      return;
    }
  } catch (error) {
    console.debug("tabs.create unavailable, falling back", error);
  }

  try {
    window.open(url, "_blank", "noopener");
  } catch (error) {
    console.warn("Failed to open external url", url, error);
  }
}

;// CONCATENATED MODULE: ./Extensions/combined/ryd.changelog.js


initChangelogPage();

/******/ })()
;