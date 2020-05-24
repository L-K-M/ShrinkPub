// This file is required by the index.html file and will
// be executed in the renderer process for that window.
// No Node.js APIs are available in this process because
// `nodeIntegration` is turned off. Use `preload.js` to
// selectively enable features needed in the rendering
// process.
(function () {	
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
		
		for (let file of e.dataTransfer.files) {
			let id = window.addFile(file);
			window.handleDrop({
				'id': id,
				'file': file
			});
		}
		return false;
	};
	
	
	window.log = function(level, message, f) {
		let el = document.getElementById(f.id);
		el.title = message;
		
		if(level == "error") {
			let icon = el.querySelector(".icon");
			icon.src = "./img/fail.png";
		}
		if(level == "success") {
			let icon = el.querySelector(".icon");
			icon.src = "./img/success.png";
		}
		
		let metaInfo = el.querySelector(".metaInfo");
		metaInfo.textContent = message;
	}

	let idCounter = 0;
	window.addFile = function(file) {
		let filelist = document.getElementById('filelist');
		let div = document.createElement("div");
		
		idCounter++;
		div.id = 'file-'+idCounter;
		
		if(filelist.children.length % 2) div.classList.add("zebra");
		div.classList.add("epubFile");
	
		{
			let icon = document.createElement("img");
			icon.classList.add("icon");
			icon.src = "./img/spinner.svg";
			div.appendChild(icon);
		
			let texts = document.createElement("div");
			texts.classList.add("texts");
			{
				let fileName = document.createElement("div");
				fileName.classList.add("fileName");
				fileName.textContent = file.name;
				texts.appendChild(fileName);
	
				let metaInfo = document.createElement("div");
				metaInfo.classList.add("metaInfo");
				metaInfo.textContent = file.size;
				texts.appendChild(metaInfo);
			}
			div.appendChild(texts);
		}
		filelist.appendChild(div);
		window.scrollTo(0,document.body.scrollHeight);
		return div.id;
	}
})();