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
const gulp = require('gulp');
const image = require('gulp-image');
const execFile = require('child_process').execFile;
const pngcrushPath = require('pngcrush-bin').path;
const debug = require('gulp-debug');
var nativeImage = require('electron').nativeImage


var _setImmediate = setImmediate;
process.once('loaded', function() {
  global.setImmediate = _setImmediate;
});

window.handleDropOnAppIcon = function(args) {
  let files = []
  for (let i = 1; i < args.length; i++) {
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
window.handleDrop = function(f) {
  if (f.file.type == "application/epub+zip") {
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

function toMb(size) {
  return (parseInt(size / 1000.0) / 1000);
}

function startFile(f) {
  decompress(f, function(path) {
    crunchImages(path, f, function() {
      createEpub(f, path, function(newFilePath) {
        let oldSize = f.file.size;
        let newSize = fs.statSync(newFilePath).size;
        let info = "Original size: " +
          toMb(oldSize) +
          "Mb, new size: " +
          toMb(newSize) +
          "Mb, stored at " + newFilePath
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
  while (fs.existsSync(unzipped_path)) {
    unzipped_path = f.file.path + '_unzipped_' + counter;
    counter++;
  }

  // Add the error event listener
  unzipper.on('error', function(err) {
    fail("Error during unzipping: " + err, f);
  });

  // Notify when everything is extracted
  unzipper.on('extract', function() {
    log('Finished unzipping', f);
    callback(unzipped_path);
  });

  // Notify "progress" of the decompressed files
  unzipper.on('progress', function(fileIndex, fileCount) {
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
  if (!pngcrushPath) return;
  execFile(pngcrushPath, ['-reduce', '-brute', filePath], function(result) {
    debuglog(result);
    succeed('PNG Crushed', f);
  });
}
async function compressPNG(filePath, path, f) {
  let preSize = fs.statSync(filePath).size;

  // workaround to windows path bug https://github.com/imagemin/imagemin/issues/352
  filePath = filePath.replace(/\\/g, "/");
  path = path.replace(/\\/g, "/");

  let q = [0.2, 0.4];
  if (compressionQuality == 'veryhigh') {
    q = [0.7, 0.9];
  } else if (compressionQuality == 'high') {
    q = [0.5, 0.7];
  } else if (compressionQuality == 'medium') {
    q = [0.3, 0.5];
  } else if (compressionQuality == 'low') {
    q = [0.1, 0.3];
  } else if (compressionQuality == 'verylow') {
    q = [0.1, 0.2];
  } else if (compressionQuality == 'terrible') {
    q = [0.1, 0.1];
  } else if (compressionQuality == 'atrocious') {
    q = [0.01, 0.01];
  }


  var files = await imagemin([filePath], {
    destination: path,
    plugins: [
      imageminPngquant({
        quality: q,
        speed: 1
      })
    ]
  });
  let postSize = fs.statSync(filePath).size;
  let fileName = filePath.substr(filePath.lastIndexOf("/") + 1);
  log("Compressed " + fileName + " from " + toMb(preSize) + "MB to " + toMb(postSize) + "MB", f);
  /*
  if(preSize - fs.statSync(filePath).size < 100) {
      // try again
      gulp.task('image', function () {
        gulp.src(filePath)
          .pipe(debug({title:'file', minimal:false, logger: debuglog}))
          .pipe(image())
          .pipe(debug({title:'fil2', minimal:false, logger: debuglog}))
          .pipe(gulp.dest(path))
          .pipe(debug({title:'file3', minimal:false, logger: debuglog}));
      });

      if(preSize - fs.statSync(filePath).size < 100) {
        debuglog("try yet again with crushpng")
        crushPNG(file);
      }
      debuglog(fs.statSync(filePath).size);
  }
  */
}
async function compressJPG(filePath, path, f) {
  // workaround to windows path bug https://github.com/imagemin/imagemin/issues/352
  filePath = filePath.replace(/\\/g, "/");
  path = path.replace(/\\/g, "/");

  let q ={};
  if (compressionQuality == 'veryhigh') {
    q = {
      accurate: true,
      quality: compressionQuality,
      strip: true,
      resize:false
    };
  } else if (compressionQuality == 'high') {
    q = {
      accurate: true,
      quality: compressionQuality,
      strip: true,
      resize:false
    };
  } else if (compressionQuality == 'medium') {
    q = {
      accurate: true,
      quality: compressionQuality,
      strip: true,
      resize:false
    };
  } else if (compressionQuality == 'low') {
    q ={
      accurate: true,
      quality: compressionQuality,
      strip: true,
      resize:1200
    };
  } else if (compressionQuality == 'verylow') {
    q = {
      accurate: true,
      target: 50,
      max: 55,
      min: 30,
      strip: true,
      resize:900
    };
  } else if (compressionQuality == 'terrible') {
    q = {
      accurate: true,
      target: 30,
      max: 40,
      min: 20,
      strip: true,
      resize:700
    };
  } else if (compressionQuality == 'atrocious') {
    q = {
      accurate: false,
      target: 10,
      max: 20,
      min: 5,
      strip: true,
      resize:500
    };
  }

  let preSize = fs.statSync(filePath).size;
  if(q.resize) {
    let natImg = nativeImage.createFromPath(filePath);
    let preSize = natImg.getSize();
    if(preSize.width > q.resize) {
      natImg = natImg.resize({width:q.resize});
    let postSize = natImg.getSize();
      // save it as a png file
        console.log(filePath.toLowerCase());
      if(filePath.toLowerCase().endsWith("png")) {
        await fs.writeFile(filePath, natImg.toPNG(), (error) => {
          if (error) throw error;
        });
      } else if(filePath.toLowerCase().endsWith("jpg") || filePath.toLowerCase().endsWith("jpeg")) {
        await fs.writeFile(filePath, natImg.toJPEG(80), (error) => {
          if (error) throw error;
        });
      }
    }
  }

  try {
    await imagemin([filePath], {
      destination: path,
      plugins: [
        imageminJpegRecompress(q)
      ]
    });
  } catch (e) {
    fail("Can't compress JPG " + e, f);
  }

  let postSize = fs.statSync(filePath).size;
  let fileName = filePath.substr(filePath.lastIndexOf("/"));
  log("Compressed " + fileName + " from " + toMb(preSize) + "MB to " + toMb(postSize) + "MB", f);
}
async function compressJPG2(filePath, path, f) {
  // workaround to windows path bug https://github.com/imagemin/imagemin/issues/352
  filePath = filePath.replace(/\\/g, "/");
  path = path.replace(/\\/g, "/");

  let preSize = fs.statSync(filePath).size;
  await imagemin([filePath], path, {
    use: [
      imageminMozjpeg({
        quality: 50
      })
    ]
  });
  let postSize = fs.statSync(filePath).size;
  let fileName = filePath.substr(filePath.lastIndexOf("/"));
  log("Compressed " + fileName + " from " + toMb(preSize) + "MB to " + toMb(postSize) + "MB", f);
}
async function recursiveCompression(path, promises, f) {
  const readdir = util.promisify(fs.readdir);

  let items = await readdir(path);
  for (var i = 0; i < items.length; i++) {
    let newFilePath = pathUtils.join(path, items[i]);
    debuglog(newFilePath);
    debuglog(path);

    let stats = fs.lstatSync(newFilePath);
    if (stats.isDirectory()) {
      await recursiveCompression(newFilePath, promises, f);
    } else if (stats.isFile()) {
      try {
        if (newFilePath.toLowerCase().endsWith(".png")) {
          debuglog("============ PNG Compressing " + newFilePath, f);
          await compressPNG(newFilePath, path, f);
        }
        if (newFilePath.toLowerCase().endsWith(".jpg") || newFilePath.toLowerCase().endsWith(".jpeg")) {
          debuglog("============ JPG Compressing " + newFilePath, f);
          let preSize = fs.statSync(newFilePath).size;
          await compressJPG(newFilePath, path, f);
          let postSize = fs.statSync(newFilePath).size;
          if (preSize <= postSize) await compressJPG2(newFilePath, path, f);
        }
      } catch (e) {
        debuglog(e);
        log("Error compressing " + newFilePath, f);
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
  while (fs.existsSync(new_file_name)) {
    new_file_name = f.file.path + '_compressed' + counter + ".epub";
    counter++;
  }
  log("Creating new Epub file at " + new_file_name, f);

  const zipStream = new compressing.zip.Stream();
  fs.readdir(unzipped_path, (err, files) => {
    files.forEach(file => {
      zipStream.addEntry(pathUtils.join(unzipped_path, file));
    })
  });
  zipStream
    .on('error', function(e) {
      fail("Error while creating epub: " + e, f)
    })
    .pipe(fs.createWriteStream(new_file_name))
    .on('error', function(e) {
      fail("Error while creating epub: " + e, f)
    })
    .on('finish', function() {
      callback(new_file_name)
    })
}
