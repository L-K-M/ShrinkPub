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
const imageminMozjpeg = require('imagemin-mozjpeg');
const pathUtils = require('path');

const execFile = require('child_process').execFile;
const pngcrushPath = require('pngcrush-bin').path;


var _setImmediate = setImmediate;
process.once('loaded', function() {
  global.setImmediate = _setImmediate;
});

window.handleDropOnAppIcon = function(args) {
    let files = []
    for(let i=1; i<args.length; i++) {
        let stat = fs.statSync(args[i]);
		let id = window.addFile(stat);
		window.handleDrop({
			'id': id,
			'file': stat
		});
    }
}

/**** IPC methods ****/

var compressionQuality = "medium";
window.setCompressionQuality = function(quality) {
	compressionQuality = quality;
}
window.handleDrop = function(f){
	if(f.file.type == "application/epub+zip") {
		startFile(f);
	} else {
		fail("I'm no Epub yet!", f);
	}
}
function fail(message, f) {
	window.log("error", message, f);
}
function succeed(message, f) {
	window.log("success", message, f);
}
function log(message, f) {
	window.log("log", message, f);
}

/**** Recompression Logic ****/

function debuglog(txt) {
  //console.log(txt);
}
function startFile(f) {
	decompress(f, function(path) {
		crunchImages(path, f, function() {
			createEpub(f, path, function(newFilePath) {
				let oldSize = f.file.size;
				let newSize = fs.statSync(newFilePath).size;
				let info = "Original size: "+
					(parseInt(oldSize / 1000.0)/1000)+
					"Mb, new size: "+
					(parseInt(newSize / 1000.0)/1000)+
					"Mb, stored at "+newFilePath
				succeed(info, f);
			})
		})
	});
}

/**** Unzipping ****/

function decompress(f, callback) {
	let unzipper = new DecompressZip(f.file.path);
	let unzipped_path = f.file.path + '_unzipped';
	let counter = 1;
	while(fs.existsSync(unzipped_path)) {
		unzipped_path = f.file.path + '_unzipped_'+counter;
		counter++;
	}

	// Add the error event listener
	unzipper.on('error', function (err) {
		fail("Error during unzipping: "+err, f);
	});

	// Notify when everything is extracted
	unzipper.on('extract', function () {
		log('Finished unzipping', f);
		callback(unzipped_path);
	});

	// Notify "progress" of the decompressed files
	unzipper.on('progress', function (fileIndex, fileCount) {
		log('Extracted file ' + (fileIndex + 1) + ' of ' + fileCount, f);
	});

	unzipper.extract({
		path: unzipped_path
	});
}

/**** Crunching Images ****/

function crunchImages(unzipped_path, f, callback) {
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
	promises.push(recursiveCompression(unzipped_path, promises, f));
	iterablePromise(promises).then(callback);
}
function crushPNG(filePath) {
  if(!pngcrushPath) return;
  execFile(pngcrushPath, ['-reduce', '-brute', filePath], function() {
    succeed('PNG Crushed');
  });
}
async function compressPNG(file, path) {
  let preSize = fs.statSync(file).size;
  debuglog("presize: "+preSize);
  await imagemin([file], {
    destination: path,
    plugins: [
      imageminPngquant({
        quality: [0.2, 0.4],
        speed: 1
      })
    ]
  });
  debuglog("postsize: "+fs.statSync(file).size);
  if(preSize - fs.statSync(file).size < 100) {
      // try again
      debuglog("try again")
      crushPNG(file);
      debuglog(fs.statSync(file).size);
  }
}
async function compressJPG(file, path) {
  let preSize = fs.statSync(file).size;
  debuglog("presize: "+preSize);
  await imagemin([file], {
    destination: path,
    plugins: [
      imageminJpegRecompress({
        accurate:true,
        quality:compressionQuality,
        strip:true
      })
    ]
  });
  debuglog("postsize: "+fs.statSync(file).size);
  if(preSize - fs.statSync(file).size < 100) {
      // try again
      debuglog("try again")
      await imagemin([file], path, {
          use: [
              imageminMozjpeg({quality: 50})
          ]
      });
      debuglog(fs.statSync(file).size);
  }
}
async function recursiveCompression(path, promises, f) {
	const readdir = util.promisify(fs.readdir);

    let items = await readdir(path);
	for (var i=0; i<items.length; i++) {
		let newFilePath = pathUtils.join(path, items[i]);
    debuglog(newFilePath);

		let stats = fs.lstatSync(newFilePath);
		if(stats.isDirectory()) {
			await recursiveCompression(newFilePath, promises, f);
		} else if(stats.isFile()) {
      try {
  			if(newFilePath.endsWith(".png")) {
          debuglog("============ PNG Compressing "+newFilePath, f);
          await compressPNG(newFilePath, path);
        }
        if(newFilePath.endsWith(".jpg") || newFilePath.endsWith(".jpeg")) {
  				debuglog("============ JPG Compressing "+newFilePath, f);
          await compressJPG(newFilePath, path);
        }
			} catch(e) {
          debuglog(e);
			    log("Error compressing "+newFilePath, f);
				// Some images may be broken and can't be compressed
				// This shouldn't stop the whole process, just ignore those
			}
		}
	}
}

/**** Rezipping ****/

function createEpub(f, unzipped_path, callback) {
	let new_file_name = f.file.path + '_compressed.epub';
	let counter = 1;
	while(fs.existsSync(new_file_name)) {
		new_file_name = f.file.path + '_compressed'+counter+".epub";
		counter++;
	}
	log("Creating new Epub file at "+new_file_name, f);

	const zipStream = new compressing.zip.Stream();
	fs.readdir(unzipped_path, (err, files) => {
		files.forEach(file => {
			zipStream.addEntry(pathUtils.join(unzipped_path, file));
		})
	});
	zipStream
	  .on('error', function(e){fail("Error while creating epub: "+e, f)})
	  .pipe(fs.createWriteStream(new_file_name))
	  .on('error', function(e){fail("Error while creating epub: "+e, f)})
	  .on('finish', function(){callback(new_file_name)})
}
