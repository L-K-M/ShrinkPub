// This file is required by the index.html file and will
// be executed in the renderer process for that window.
// No Node.js APIs are available in this process because
// `nodeIntegration` is turned off. Use `preload.js` to
// selectively enable features needed in the rendering
// process.
(function () {
	let filelist = document.getElementById('filelist');
	window.setElements(filelist);
	
	let qualitySelector = document.getElementById('qualitySelector');
	qualitySelector.onchange = function() {
		let quality = qualitySelector.options[qualitySelector.selectedIndex].value;
		window.setCompressionQuality(quality);
	}
	
	let dropzone = document.getElementById('fullPage');
	dropzone.ondragover = () => {
		if(dropzone.classList.contains("hover")) return false;
		dropzone.classList.add("hover");
		return false;
	};
	dropzone.ondragleave = () => {
		dropzone.classList.remove("hover");
		return false;
	};
	dropzone.ondragend = () => {
		dropzone.classList.remove("hover");
		return false;
	};
	dropzone.ondrop = (e) => {
		e.preventDefault();
		
		dropzone.classList.remove("hover");
		let dragHere = document.getElementById('dragHere');
		dragHere.style.display = 'none';
		
		window.handleDrop(e.dataTransfer.files);
		return false;
	};
})();