// this file contains the jellyfin client module. We will use this module to interact with the jellyfin server.
// The client module will contain the following:
// 1. A struct that will hold the base url of the jellyfin server.
// 2. A function that will create a new instance of the client struct.
// 3. A function that will get the server information.
// 4. A function that will get the server users.
// 5. A function that will get the server libraries.

use std::fmt::format;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use reqwest::{self, Response};
use serde::Serialize;
use serde::Deserialize;
use serde_json::Value;

use std::io::Cursor;
use std::io::Seek;

use crate::player::{self, Song};

use futures_util::StreamExt;

use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::Bytes;
use futures::{Stream};

use serde_yaml;

pub struct ByteStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
}

#[derive(Debug)]
pub struct Client {
    base_url: String,
    http_client: reqwest::Client,
    credentials: Option<Credentials>,
    pub access_token: String,
    user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(rename = "Username")]
    username: String,
    #[serde(rename = "Pw")]
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    version: String,
    url: String,
}

impl Client {
    pub async fn new(base_url: &str) -> Self {
        let f = std::fs::File::open("config.yaml").unwrap();
        let d: Value = serde_yaml::from_reader(f).unwrap();        

        let http_client = reqwest::Client::new();
        let _credentials = {
            // let username = std::env::var("").ok();
            // let password = std::env::var("").ok();
            let username = d["username"].as_str();
            let password = d["password"].as_str();
            match (username, password) {
                (Some(username), Some(password)) => Some(Credentials {
                    username: username.to_string(),
                    password: password.to_string(),
                }),
                _ => None,
            }
        };
        
        // println!("{}", format!("{}/Users/authenticatebyname", d["host"]).as_str());
        // without the ""
        let url: String = String::new() + &d["host"].as_str().unwrap() + "/Users/authenticatebyname";
        let response = http_client
            .post(url)
            .header("Content-Type", "text/json")
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            // .json(&Credentials {
            //     username: "".to_string(),
            //     password: "".to_string(),
            // })
            .json(&serde_json::json!({
                "Username": d["username"].as_str().unwrap(),
                "Pw": d["password"].as_str().unwrap()
            }))
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();
        if !status.is_success() {
            println!("Error authenticating. Status: {}", status);
            return Self {
                base_url: base_url.to_string(),
                http_client,
                credentials: _credentials,
                access_token: "".to_string(),
                user_id: "".to_string(),
            };
        }
            
        // get response data
        let response: Value = response.unwrap().json().await.unwrap();
        // get AccessToken
        let access_token = response["AccessToken"].as_str().unwrap();
        // println!("Access Token: {}", access_token);

        // get user id (User.Id)
        let user_id = response["User"]["Id"].as_str().unwrap();
        // println!("User Id: {}", user_id);


        // println!("{:#?}", response);
        Self {
            base_url: base_url.to_string(),
            http_client,
            credentials: _credentials,
            access_token: access_token.to_string(),
            user_id: user_id.to_string(),
        }
    }

    pub async fn artists(&self) -> Result<Vec<Artist>, reqwest::Error> {
        // let url = format!("{}/Users/{}/Artists", self.base_url, self.user_id);
        let url = format!("{}/Artists", self.base_url);
        println!("url: {}", url);

        // to send some credentials we can use the basic_auth method
        // let response = self.http_client.get(url).basic_auth(&self.credentials.username, Some(&self.credentials.password)).send().await;
        let s = format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\" Token=\"{}\"", self.access_token);
        println!("s: {}", s);
        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "SortName"),
                ("SortOrder", "Ascending"), 
                ("Recursive", "true"), 
                ("Fields", "SortName"), 
                ("ImageTypeLimit", "-1")
            ])
            .query(&[("StartIndex", "0")])
            .query(&[("Limit", "100")])
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();

        // check if response is ok
        if !response.as_ref().unwrap().status().is_success() {
            println!("Error getting artists. Status: {}", status);
            return Ok(vec![]);
        }

        // deseralize using our types
        let artists: Artists = response.unwrap().json().await.unwrap();
        // println!("{:#?}", artists);


        Ok(artists.items)
    }

    pub async fn discography(&self, id: &str) -> Result<Discography, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);

        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "Album"),
                ("SortOrder", "Ascending"),
                ("Recursive", "true"), 
                ("IncludeItemTypes", "Audio"),
                ("Fields", "Genres, DateCreated, MediaSources, ParentId"),
                ("StartIndex", "0"),
                ("ImageTypeLimit", "1"),
                ("ArtistIds", id)
            ])
            .query(&[("StartIndex", "0")])
            .query(&[("Limit", "100")])
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();

        // check if response is ok
        if !response.as_ref().unwrap().status().is_success() {
            println!("Error getting artists. Status: {}", status);
            return Ok(Discography {
                items: vec![],
            });
        }

        // artists is the json string of all artists

        // first arbitrary json
        // let artist: Value = response.unwrap().json().await.unwrap();
        // println!("{:#?}?", artist);
        let discog: Discography = response.unwrap().json().await.unwrap();
        // println!("{:#?}", discog);

        return Ok(discog);
    }

    // get json schema of all artists
    // url/Artists?enableImages=true&enableTotalRecordCount=true
    pub async fn songs(&self) -> Result<Value, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);
        // let url = format!("{}/Songs", self.base_url);
        println!("url: {}", url);


        // to send some credentials we can use the basic_auth method
        // let response = self.http_client.get(url).basic_auth(&self.credentials.username, Some(&self.credentials.password)).send().await;
        let s = format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\" Token=\"{}\"", self.access_token);
        println!("s: {}", s);
        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            // ?SortBy=Album%2CSortName&SortOrder=Ascending&IncludeItemTypes=Audio&Recursive=true&Fields=ParentId&StartIndex=0&ImageTypeLimit=1&EnableImageTypes=Primary
            .query(&[("SortBy", "Album,SortName"), ("SortOrder", "Ascending"), ("IncludeItemTypes", "Audio"), ("Recursive", "true"), ("Fields", "ParentId"), ("StartIndex", "0"), ("ImageTypeLimit", "1"), ("EnableImageTypes", "Primary")])
            .query(&[("Limit", "100")])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();

        // check if response is ok
        if !response.as_ref().unwrap().status().is_success() {
            println!("Error getting artists. Status: {}", status);
            return Ok(serde_json::json!({}));
        }

        // artists is the json string of all artists
        let songs: Value = response.unwrap().json().await.unwrap();
        
        // println!("{:#?}", songs);

        Ok(songs)
    }

    pub async fn song_info(&self, song_id: &str) -> Result<Song, reqwest::Error> {
        let url = format!("{}/Items/{}", self.base_url, song_id);
        println!("url: {}", url);

        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();

        // check if response is ok
        if !response.as_ref().unwrap().status().is_success() {
            println!("Error getting artists. Status: {}", status);
            return Ok(Song::new(0, 0, None, 0));
        }

        // artists is the json string of all artists
        let song_info: Value = response.unwrap().json().await.unwrap();
        
        // println!("SONG INFO{:#?}", song_info);

        let channels = song_info["MediaStreams"][0]["Channels"].as_u64().unwrap() as u16;
        let srate = song_info["MediaStreams"][0]["SampleRate"].as_u64().unwrap() as u32;
        let duration = song_info["RunTimeTicks"].as_u64().unwrap() as u64;
        let file_size = song_info["MediaSources"][0]["Size"].as_u64().unwrap() as u64;
        println!("Channels: {}", channels);
        println!("Sample Rate: {}", srate);
        println!("Duration: {}", duration);
        println!("File Size in bytes: {}", file_size);
        println!("File Size in MB: {}", file_size / 1024 / 1024);

        Ok(Song::new(channels, srate, Some(std::time::Duration::from_secs(duration / 10000000)), file_size))
    }

    pub async fn stream(&self) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>, reqwest::Error> {
        let url = format!("{}/Audio/{}/universal", self.base_url, "0416871eb42dd5aa5c73da6930d6028e");
        println!("url: {}", url);

        // get song info
    
        let s = format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\" Token=\"{}\"", self.access_token);
        println!("s: {}", s);
    
        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .query(&[
                ("UserId", self.user_id.to_string()),
                ("Container", "opus,webm|opus,mp3,aac,m4a|aac,m4b|aac,flac,webma,webm|webma,wav,ogg".to_string()),
                ("TranscodingContainer", "mp4".to_string()),
                ("TranscodingProtocol", "hls".to_string()),
                ("AudioCodec", "aac".to_string()),
                ("api_key", self.access_token.to_string()),
                ("StartTimeTicks", "0".to_string()),
                ("EnableRedirection", "true".to_string()),
                ("EnableRemoteMedia", "false".to_string())
            ])
            .send()
            .await?;
    
        let status = response.status();
    
        if !status.is_success() {
            println!("Error getting artists. Status: {}", status);
            // return Ok(Cursor::new(Arc::new([])));
            return Ok(Box::pin(futures::stream::empty()));
        } else {
            println!("Success getting audio stream. Status: {}", status);
        }
    
        //let content = vec![];
        // now we need to stream the data. For debugging just make a loop and print the data
        let mut stream = response.bytes_stream();
        // while let Some(item) = stream.next().await {
        //     // println!("Chunk: {:?}", item?);
        //     //println!("Chunk size: {:?}", item?.len());
        // }
        //println!("Content: {:?}", content.len());
        // Ok(Cursor::new(Arc::from(content.as_ref())))
        Ok(Box::pin(stream))

        // this is nice, but it gets the entire file at once. We need to stream it! So here returns a cursor that will stream the data. We can't just call .bytes() on the response because it will consume the response. We need to stream the data.
        // let content = response.bytes().await?; // this is bad
        // let content = response.bytes().await?;
        // Ok(Cursor::new(Arc::from(content.as_ref())))
    }

    // pub async fn stream(buffer: Arc<Mutex<StreamBuffer>>, base_url: &str, access_token: &str, user_id: &str, http_client: &reqwest::Client) -> Result<(), reqwest::Error> {
    //     let url = format!("{}/Audio/{}/universal", base_url, "2f039eccf11d82f21a2b74a6954ddef2");
    //     println!("url: {}", url);

    //     let response = http_client
    //         .get(&url)
    //         .header("X-MediaBrowser-Token", access_token.to_string())
    //         .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
    //         .header("Content-Type", "text/json")
    //         .query(&[
    //             ("UserId", user_id.to_string()),
    //             ("Container", "opus,webm|opus,mp3,aac,m4a|aac,m4b|aac,flac,webma,webm|webma,wav,ogg".to_string()),
    //             ("TranscodingContainer", "mp4".to_string()),
    //             ("TranscodingProtocol", "hls".to_string()),
    //             ("AudioCodec", "aac".to_string()),
    //             ("api_key", access_token.to_string()),
    //             ("StartTimeTicks", "0".to_string()),
    //             ("EnableRedirection", "true".to_string()),
    //             ("EnableRemoteMedia", "false".to_string())
    //         ])
    //         .send()
    //         .await?;

    //     if !response.status().is_success() {
    //         println!("Error getting audio stream. Status: {}", response.status());
    //         return Ok(());
    //     }

    //     let mut stream_buffer = buffer.lock().unwrap();
    //     let content = response.bytes().await?;

    //     for &byte in content.iter() {
    //         stream_buffer.data.push_back(byte);
    //     }

    //     Ok(())
    // }


}




/// TYPES ///
/// 
/// All the jellyfin types will be defined here. These types will be used to interact with the jellyfin server.

/// ARTIST
/* {
  "Name": "Flam",
  "ServerId": "97a9003303d7461395074680d9046935",
  "Id": "a9b08901ce0884038ef2ab824e4783b5",
  "SortName": "flam",
  "ChannelId": null,
  "RunTimeTicks": 4505260770,
  "Type": "MusicArtist",
  "UserData": {
    "PlaybackPositionTicks": 0,
    "PlayCount": 0,
    "IsFavorite": false,
    "Played": false,
    "Key": "Artist-Musicbrainz-622c87fa-dc5e-45a3-9693-76933d4c6619"
  },
  "ImageTags": {},
  "BackdropImageTags": [],
  "ImageBlurHashes": {},
  "LocationType": "FileSystem",
  "MediaType": "Unknown"
} */
#[derive(Debug, Serialize, Deserialize)]
pub struct Artists {
    #[serde(rename = "Items")]
    items: Vec<Artist>,
    #[serde(rename = "StartIndex")]
    start_index: u64,
    #[serde(rename = "TotalRecordCount")]
    total_record_count: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Artist {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "SortName")]
    sort_name: String,
    #[serde(rename = "RunTimeTicks")]
    run_time_ticks: u64,
    #[serde(rename = "Type")]
    type_: String,
    #[serde(rename = "UserData")]
    user_data: UserData,
    #[serde(rename = "ImageTags")]
    image_tags: serde_json::Value,
    #[serde(rename = "ImageBlurHashes")]
    image_blur_hashes: serde_json::Value,
    #[serde(rename = "LocationType")]
    location_type: String,
    #[serde(rename = "MediaType")]
    media_type: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UserData {
    #[serde(rename = "PlaybackPositionTicks")]
    playback_position_ticks: u64,
    #[serde(rename = "PlayCount")]
    play_count: u64,
    #[serde(rename = "IsFavorite")]
    is_favorite: bool,
    #[serde(rename = "Played")]
    played: bool,
    #[serde(rename = "Key")]
    key: String,
}

/// DISCOGRAPHY
/// 
/// The goal here is to mimic behavior of CMUS and get the whole discography of an artist.
/// We query jellyfin for all songs by an artist sorted by album and sort name.
/// Later we group them nicely by album.

/*
Object {
    "Album": String("Cardan [EP]"),
    "AlbumArtist": String("Agar Agar"),
    "AlbumArtists": Array [
        Object {
            "Id": String("c910b835045265897c9b1e30417937c8"),
            "Name": String("Agar Agar"),
        },
    ],
    "AlbumId": String("e66386bd52e9e13bcd53fefbe4dbfe80"),
    "AlbumPrimaryImageTag": String("728e73b82a9103d8d3bd46615f7c0786"),
    "ArtistItems": Array [
        Object {
            "Id": String("c910b835045265897c9b1e30417937c8"),
            "Name": String("Agar Agar"),
        },
    ],
    "Artists": Array [
        String("Agar Agar"),
    ],
    "BackdropImageTags": Array [],
    "ChannelId": Null,
    "DateCreated": String("2024-03-12T12:41:07.2583951Z"),
    "GenreItems": Array [
        Object {
            "Id": String("5897c94bfe512270b15fa7e6088e94d0"),
            "Name": String("Synthpop"),
        },
    ],
    "Genres": Array [
        String("Synthpop"),
    ],
    "HasLyrics": Bool(true),
    "Id": String("b26c12ffca74316396cb3d366a7f09f5"),
    "ImageBlurHashes": Object {
        "Backdrop": Object {
            "ea9ad04d014bd8317aa784ffb5676eac": String("W797hQ?bf7ofxuWU?b~qxut6t7M|-;xu%Mayj[xu-:j[xuRjRjt7"),
        },
        "Primary": Object {
            "222d9d1264b6994621fe99bb78047348": String("eQG*]WD+VD=|H?CmIoIotlM|Q,n%R*oeozVXjY$$n%WBMds.tRW=ni"),
            "728e73b82a9103d8d3bd46615f7c0786": String("eQG*]WD+VD=|H?CmIoIotlM|Q,n%R*oeozVXjY$$n%WBMds.tRW=ni"),
        },
    },
    "ImageTags": Object {
        "Primary": String("222d9d1264b6994621fe99bb78047348"),
    },
    "IndexNumber": Number(3),
    "IsFolder": Bool(false),
    "LocationType": String("FileSystem"),
    "MediaSources": Array [
        Object {
            "Bitrate": Number(321847),
            "Container": String("mp3"),
            "DefaultAudioStreamIndex": Number(0),
            "ETag": String("23dab11df466604c0b0cade1f8f814da"),
            "Formats": Array [],
            "GenPtsInput": Bool(false),
            "Id": String("b26c12ffca74316396cb3d366a7f09f5"),
            "IgnoreDts": Bool(false),
            "IgnoreIndex": Bool(false),
            "IsInfiniteStream": Bool(false),
            "IsRemote": Bool(false),
            "MediaAttachments": Array [],
            "MediaStreams": Array [
                Object {
                    "AudioSpatialFormat": String("None"),
                    "BitRate": Number(320000),
                    "ChannelLayout": String("stereo"),
                    "Channels": Number(2),
                    "Codec": String("mp3"),
                    "DisplayTitle": String("MP3 - Stereo"),
                    "Index": Number(0),
                    "IsAVC": Bool(false),
                    "IsDefault": Bool(false),
                    "IsExternal": Bool(false),
                    "IsForced": Bool(false),
                    "IsHearingImpaired": Bool(false),
                    "IsInterlaced": Bool(false),
                    "IsTextSubtitleStream": Bool(false),
                    "Level": Number(0),
                    "SampleRate": Number(44100),
                    "SupportsExternalStream": Bool(false),
                    "TimeBase": String("1/14112000"),
                    "Type": String("Audio"),
                    "VideoRange": String("Unknown"),
                    "VideoRangeType": String("Unknown"),
                },
                Object {
                    "AspectRatio": String("1:1"),
                    "AudioSpatialFormat": String("None"),
                    "BitDepth": Number(8),
                    "Codec": String("mjpeg"),
                    "ColorSpace": String("bt470bg"),
                    "Comment": String("Cover (front)"),
                    "Height": Number(500),
                    "Index": Number(1),
                    "IsAVC": Bool(false),
                    "IsAnamorphic": Bool(false),
                    "IsDefault": Bool(false),
                    "IsExternal": Bool(false),
                    "IsForced": Bool(false),
                    "IsHearingImpaired": Bool(false),
                    "IsInterlaced": Bool(false),
                    "IsTextSubtitleStream": Bool(false),
                    "Level": Number(-99),
                    "PixelFormat": String("yuvj420p"),
                    "Profile": String("Baseline"),
                    "RealFrameRate": Number(90000),
                    "RefFrames": Number(1),
                    "SupportsExternalStream": Bool(false),
                    "TimeBase": String("1/90000"),
                    "Type": String("EmbeddedImage"),
                    "VideoRange": String("Unknown"),
                    "VideoRangeType": String("Unknown"),
                    "Width": Number(500),
                },
                Object {
                    "AudioSpatialFormat": String("None"),
                    "Index": Number(2),
                    "IsDefault": Bool(false),
                    "IsExternal": Bool(false),
                    "IsForced": Bool(false),
                    "IsHearingImpaired": Bool(false),
                    "IsInterlaced": Bool(false),
                    "IsTextSubtitleStream": Bool(false),
                    "Path": String("/data/music/Agar Agar/Cardan/03 - Cuidado, Peligro, Eclipse.txt"),
                    "SupportsExternalStream": Bool(false),
                    "Type": String("Lyric"),
                    "VideoRange": String("Unknown"),
                    "VideoRangeType": String("Unknown"),
                },
            ],
            "Name": String("03 - Cuidado, Peligro, Eclipse"),
            "Path": String("/data/music/Agar Agar/Cardan/03 - Cuidado, Peligro, Eclipse.mp3"),
            "Protocol": String("File"),
            "ReadAtNativeFramerate": Bool(false),
            "RequiredHttpHeaders": Object {},
            "RequiresClosing": Bool(false),
            "RequiresLooping": Bool(false),
            "RequiresOpening": Bool(false),
            "RunTimeTicks": Number(3600979590),
            "Size": Number(14487065),
            "SupportsDirectPlay": Bool(true),
            "SupportsDirectStream": Bool(true),
            "SupportsProbing": Bool(true),
            "SupportsTranscoding": Bool(true),
            "TranscodingSubProtocol": String("http"),
            "Type": String("Default"),
        },
    ],
    "MediaType": String("Audio"),
    "Name": String("Cuidado, Peligro, Eclipse"),
    "NormalizationGain": Number(-10.45),
    "ParentBackdropImageTags": Array [
        String("ea9ad04d014bd8317aa784ffb5676eac"),
    ],
    "ParentBackdropItemId": String("c910b835045265897c9b1e30417937c8"),
    "ParentId": String("e66386bd52e9e13bcd53fefbe4dbfe80"),
    "ParentIndexNumber": Number(0),
    "PremiereDate": String("2016-01-01T00:00:00.0000000Z"),
    "ProductionYear": Number(2016),
    "RunTimeTicks": Number(3600979590),
    "ServerId": String("97a9003303d7461395074680d9046935"),
    "Type": String("Audio"),
    "UserData": Object {
        "IsFavorite": Bool(false),
        "Key": String("Agar Agar-Cardan [EP]-0000-0003Cuidado, Peligro, Eclipse"),
        "PlayCount": Number(0),
        "PlaybackPositionTicks": Number(0),
        "Played": Bool(false),
    },
}, */

#[derive(Debug, Serialize, Deserialize)]
pub struct Discography {
    #[serde(rename = "Items")]
    items: Vec<DiscographySong>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscographyAlbum {
    songs: Vec<DiscographySong>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscographySongUserData {
    #[serde(rename = "PlaybackPositionTicks")]
    playback_position_ticks: u64,
    #[serde(rename = "PlayCount")]
    play_count: u64,
    #[serde(rename = "IsFavorite")]
    is_favorite: bool,
    #[serde(rename = "Played")]
    played: bool,
    #[serde(rename = "Key")]
    key: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiscographySong {
    #[serde(rename = "Album")]
    album: String,
    #[serde(rename = "AlbumArtist")]
    album_artist: String,
    // #[serde(rename = "AlbumArtists")]
    // album_artists: Vec<Artist>,
    #[serde(rename = "AlbumId")]
    album_id: String,
    #[serde(rename = "AlbumPrimaryImageTag")]
    album_primary_image_tag: String,
    // #[serde(rename = "ArtistItems")]
    // artist_items: Vec<Artist>,
    // #[serde(rename = "Artists")]
    // artists: Vec<String>,
    #[serde(rename = "BackdropImageTags")]
    backdrop_image_tags: Vec<String>,
    #[serde(rename = "ChannelId")]
    channel_id: Option<String>,
    #[serde(rename = "DateCreated")]
    date_created: String,
    // #[serde(rename = "GenreItems")]
    // genre_items: Vec<Genre>,
    #[serde(rename = "Genres")]
    genres: Vec<String>,
    #[serde(rename = "HasLyrics")]
    has_lyrics: bool,
    #[serde(rename = "Id")]
    id: String,
    // #[serde(rename = "ImageBlurHashes")]
    // image_blur_hashes: ImageBlurHashes,
    // #[serde(rename = "ImageTags")]
    // image_tags: ImageTags,
    // #[serde(rename = "IndexNumber")]
    // index_number: u64,
    #[serde(rename = "IsFolder")]
    is_folder: bool,
    // #[serde(rename = "LocationType")]
    // location_type: String,
    // #[serde(rename = "MediaSources")]
    // media_sources: Vec<MediaSource>, // ignore for now, probably new route
    #[serde(rename = "MediaType")]
    media_type: String,
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "NormalizationGain")]
    normalization_gain: f64,
    #[serde(rename = "ParentBackdropImageTags")]
    parent_backdrop_image_tags: Vec<String>,
    #[serde(rename = "ParentBackdropItemId")]
    parent_backdrop_item_id: String,
    #[serde(rename = "ParentId")]
    parent_id: String,
    #[serde(rename = "ParentIndexNumber")]
    parent_index_number: u64,
    #[serde(rename = "PremiereDate")]
    premiere_date: String,
    #[serde(rename = "ProductionYear")]
    production_year: u64,
    #[serde(rename = "RunTimeTicks")]
    run_time_ticks: u64,
    #[serde(rename = "ServerId")]
    server_id: String,
    // #[serde(rename = "Type")]
    // type_: String,
    #[serde(rename = "UserData")]
    user_data: DiscographySongUserData,
}