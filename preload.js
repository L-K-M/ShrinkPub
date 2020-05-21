// All of the Node.js APIs are available in the preload process.
// It has the same sandbox as a Chrome extension.
const DecompressZip = require('decompress-zip')
const fs = require('fs')
const imagemin = require('imagemin');
const imageminJpegtran = require('imagemin-jpegtran');
const imageminPngquant = require('imagemin-pngquant');
const util = require('util');
const compressing = require('compressing');
const imageminJpegRecompress = require('imagemin-jpeg-recompress');

var domElements = {};
window.setElements = function(filelist) {
	domElements.filelist = filelist;
}
var compressionQuality = "medium";
window.setCompressionQuality = function(quality) {
	compressionQuality = quality;
}


window.handleDrop = function(files){
	for (let f of files) {
		let el = addFile(f);
		if(f.type == "application/epub+zip") {
			startFile(f, el);
		} else {
			fail("I'm no Epub yet!", el);
		}
	}
}
function fail(error, el) {
	let icon = el.querySelector(".icon");
	icon.src = "./img/fail.png";

	let metaInfo = el.querySelector(".metaInfo");
	metaInfo.textContent = error;
}
function succeed(message, el) {
	let icon = el.querySelector(".icon");
	icon.src = "./img/success.png";

	let metaInfo = el.querySelector(".metaInfo");
	metaInfo.textContent = message;
}
function log(text, el) {
	let metaInfo = el.querySelector(".metaInfo");
	metaInfo.textContent = text;
}

function addFile(f) {
	let div = document.createElement("div");
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
			fileName.textContent = f.name;
			texts.appendChild(fileName);
	
			let metaInfo = document.createElement("div");
			metaInfo.classList.add("metaInfo");
			metaInfo.textContent = f.size;
			texts.appendChild(metaInfo);
		}
		div.appendChild(texts);
	}
	domElements.filelist.appendChild(div);
	return div;
}

function startFile(f, el) {
	decompress(f, el, function(path) {
		crunchImages(path, el, function() {
			createEpub(f.path, path, el, function(newFilePath) {
				let oldSize = f.size;
				let newSize = fs.statSync(newFilePath).size;
				let info = "Original size: "+
					(parseInt(oldSize / 1000.0)/1000)+
					"Mb, new size: "+
					(parseInt(newSize / 1000.0)/1000)+
					"Mb, stored at "+newFilePath
				succeed(info, el);
			})
		})
	});
}

/**** Unzipping ****/

function decompress(f, el, callback) {
	let unzipper = new DecompressZip(f.path);
	let unzipped_path = f.path + '_unzipped';
	let counter = 1;
	while(fs.existsSync(unzipped_path)) {
		unzipped_path = f.path + '_unzipped_'+counter;
		counter++;
	}
	
	// Add the error event listener
	unzipper.on('error', function (err) {
		fail("Error during unzipping: "+err, el);
	});

	// Notify when everything is extracted
	unzipper.on('extract', function () {
		log('Finished unzipping', el);
		callback(unzipped_path);
	});

	// Notify "progress" of the decompressed files
	unzipper.on('progress', function (fileIndex, fileCount) {
		log('Extracted file ' + (fileIndex + 1) + ' of ' + fileCount, el);
	});

	unzipper.extract({
		path: unzipped_path
	});
}

/**** Crunching Images ****/

function crunchImages(unzipped_path, el, callback) {
	function iterablePromise(iterable) {
	  return Promise.all(iterable).then(function(resolvedIterable) {
		if (iterable.length != resolvedIterable.length) {
		  // The list of promises or values changed. Return a new Promise.
		  // The original promise won't resolve until the new one does.
		  return iterablePromise(iterable);
		}
		// The list of promises or values stayed the same.
		// Return results immediately.
		return resolvedIterable;
	  });
	}
	
	let promises = [];
	promises.push(recursiveCompression(unzipped_path, promises, el));
	iterablePromise(promises).then(callback);
}
async function recursiveCompression(path, promises, el) {
	const readdir = util.promisify(fs.readdir);
	
    let items = await readdir(path);
	for (var i=0; i<items.length; i++) {
		let newFilePath = path+"/"+items[i];
		
		let stats = fs.lstatSync(newFilePath);
		if(stats.isDirectory()) {
			await recursiveCompression(newFilePath, promises, el);
		} else if(stats.isFile()) {
			if(newFilePath.endsWith(".png") || newFilePath.endsWith(".jpg") || newFilePath.endsWith(".jpeg")) {
				log("Compressing "+newFilePath, el);
				try {
					await imagemin([newFilePath], {
						destination: path,
						plugins: [
							imageminJpegRecompress({
								accurate:true,
								quality:compressionQuality
							}),
							imageminPngquant({
								quality: [0.5, 0.8]
							})
						]
					});
				} catch(e) {
					// Some images may be broken and can't be compressed
					// This shouldn't stop the whole process, just ignore those
				}
			}
		}
	}
}

/**** Rezipping ****/

function createEpub(epubName, unzipped_path, el, callback) {
	let new_file_name = epubName + '_compressed.epub';
	let counter = 1;
	while(fs.existsSync(new_file_name)) {
		new_file_name = epubName + '_compressed'+counter+".epub";
		counter++;
	}
	log("Creating new Epub file at "+epubName, el);
	
	const zipStream = new compressing.zip.Stream();
	fs.readdir(unzipped_path, (err, files) => { 
		files.forEach(file => { 
			zipStream.addEntry(unzipped_path+"/"+file);
		}) 
	});
	zipStream
	  .on('error', function(e){fail("Error while creating epub: "+e, el)})
	  .pipe(fs.createWriteStream(new_file_name))
	  .on('error', function(e){fail("Error while creating epub: "+e, el)})
	  .on('finish', function(){callback(new_file_name)})
}