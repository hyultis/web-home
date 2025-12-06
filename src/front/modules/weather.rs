use std::ops::Deref;
use js_sys::Function;
use leptos::prelude::{ClassAttribute, CollectView, Effect, ElementChild, Get, GetUntracked, OnAttribute, PropAttribute, Read, ReadUntracked, StyleAttribute, Update};
use leptos::prelude::{AnyView, ArcRwSignal, IntoAny, RwSignal};
use leptos::view;
use leptos_use::{use_geolocation, UseGeolocationReturn};
use serde::{Deserialize, Serialize};
use simd_json::borrowed::Value;
use simd_json::prelude::ValueAsScalar;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{window, Geolocation, Position, PositionError, Response};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, Cache, Cacheable};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::translate::Translate;
use crate::HWebTrace;
use leptos::prelude::OnTargetAttribute;
use time::UtcDateTime;
use wasm_bindgen::prelude::Closure;

#[derive(Serialize,Deserialize,Debug)]
#[derive(Clone)]
struct WeatherConfig
{
	pub latitude: f64,
	pub longitude: f64,
	pub maxday: u8,
}
impl Default for WeatherConfig
{
	fn default() -> Self
	{
		Self {
			latitude: 0.0,
			longitude: 0.0,
			maxday: 3,
		}
	}
}

#[derive(Serialize, Deserialize, Debug)]
#[derive(Default)]
pub struct Weather
{
	config: ArcRwSignal<WeatherConfig>,
	#[serde(skip_serializing,skip_deserializing)]
	weatherContent: ArcRwSignal<Option<WeatherApiResult>>,
	_update: ArcRwSignal<Cache>,
	_sended: ArcRwSignal<Cache>,
}

impl Weather
{
	pub fn celsiusToColor(temp: f64) -> String
	{
		let (h, s, l) = Self::temp_to_hsl(temp);
		format!("color: hsl({:.0}deg, {:.0}%, {:.0}%)", h, s, l)
	}

	fn lerp(a: f64, b: f64, t: f64) -> f64 {
		a + (b - a) * t
	}

	pub fn temp_to_hsl(temp: f64) -> (f64, f64, f64) {
		// H en degrés, S et L en pourcentage (0-100)
		let (h1, h2, t_min, t_max) = if temp < 0.0 {
			(220.0, 200.0, -20.0, 0.0)       // Bleu nuit → Bleu
		} else if temp < 10.0 {
			(200.0, 180.0, 0.0, 10.0)        // Bleu → Cyan
		} else if temp < 20.0 {
			(180.0, 60.0, 10.0, 20.0)        // Cyan → Jaune
		} else if temp < 30.0 {
			(60.0, 35.0, 20.0, 30.0)         // Jaune → Orange
		} else if temp < 40.0 {
			(35.0, 0.0, 30.0, 40.0)          // Orange → Rouge
		} else {
			(0.0, 290.0, 40.0, 60.0)         // Rouge → Pourpre
		};

		let t = ((temp - t_min) / (t_max - t_min)).clamp(0.0, 1.0);

		let h = Self::lerp(h1, h2, t);
		let s = 70.0;   // saturation fixe
		let l = 50.0;   // luminosité fixe

		(h, s, l)
	}

}

impl Backable for Weather
{
	fn typeModule(&self) -> String {
		"WEATHER".to_string()
	}

	fn draw(&self, editMode: RwSignal<bool>, moduleActions: ModuleActionFn, currentName: String) -> AnyView {
		HWebTrace!("draw weather");
		let config = self.config.clone();
		let refreshContent = self.weatherContent.clone();
		Effect::new(move ||{
			let config = config.clone();
			let refreshContent = refreshContent.clone();
			spawn_local(async move {
				sync_weather_api(config,refreshContent).await;
			});
		});
		/*let UseGeolocationReturn {
			coords,
			error,
			..
		} = use_geolocation();*/

		view!{{
			if(editMode.get())
			{
				let configLocate = self.config.clone();
				let locateFn = move |_| {

					let Some(window) = window() else {return};
					let navigator = window.navigator();

		            let Ok(geolocation) = navigator.geolocation() else {return};

					let configLocate = configLocate.clone();
					let on_success = Closure::once(move |pos: Position| {
						HWebTrace!("success : {:?}", pos.coords());
						configLocate.update(|conf| {
							conf.longitude = pos.coords().longitude();
							conf.latitude = pos.coords().latitude();
						});
				    });

				    // ERROR
				    let on_error = Closure::once(move |err: PositionError| {
						HWebTrace!("error : {:?}", err);
				        // TODO
				    });

					let _ = geolocation.get_current_position_with_error_callback(on_success.as_ref().unchecked_ref(), Some(on_error.as_ref().unchecked_ref()));

				    on_success.forget();
				    on_error.forget();
				};

				let configLatitude = self.config.clone();
				let configLongitude = self.config.clone();
				let configMaxDay = self.config.clone();
				let cacheTitle = self._update.clone();
				let cacheLink = self._update.clone();
				let cacheMax = self._update.clone();
				view!{
					<label for="weather_latitude"><Translate key="module_weather_latitude"/></label><input type="text" name="weather_latitude" prop:value={configLatitude.get().latitude} on:input:target=move |ev| {
						configLatitude.update(|inner|inner.latitude = ev.target().value().parse::<f64>().unwrap_or(0.0));
						cacheTitle.update(|cache| cache.update());
					} />
					<label for="weather_longitude"><Translate key="module_weather_longitude"/></label><input type="text" name="weather_longitude" prop:value={configLongitude.get().longitude}  on:input:target=move |ev| {
						configLongitude.update(|inner|inner.longitude = ev.target().value().parse::<f64>().unwrap_or(0.0));
						cacheLink.update(|cache| cache.update());
					}/><br/>
					<button on:click={locateFn}><Translate key="module_weather_locate"/></button>
					<label for="weather_maxday"><Translate key="module_weather_maxday"/></label><input type="number" min="1" max="7" name="weather_maxday" prop:value={configMaxDay.get().maxday}  on:input:target=move |ev| {
						configMaxDay.update(|inner|inner.maxday = ev.target().value().parse::<u8>().unwrap_or(3));
						cacheMax.update(|cache| cache.update());
					}/><br/>
				}.into_any()
			}
			else
			{
				view!{<div class="module_weather">{
					self.weatherContent.get().map(|haveContent| {
						let units = haveContent.unit.clone();
						haveContent.days.iter().map(|days| {
							let date = UtcDateTime::from_unix_timestamp(days.timestampDay as i64).unwrap_or(UtcDateTime::now());
							view!{
								<div class="day">
									{format!("{:0>2}",date.day())}/{format!("{:0>2}",date.month() as u8)}<br/>
									<img src={format!("weather/{}.png",days.codeIntoImg())} alt={days.codeIntoImg()} /><br/>
									<Translate key={days.codeIntoTranslate()}/><br/>
									<span style={Self::celsiusToColor(days.temp_min)}>{days.temp_min}{units.clone().temp}</span>{" - "}<span style={Self::celsiusToColor(days.temp_max)}>{days.temp_max}{units.clone().temp}</span><br/>
									<i class="iconoir-wind"/>{" "}{days.wind}{units.clone().wind}{" - "}<i class="iconoir-heavy-rain"/>{" "}{days.precipitation}{units.clone().precipitation}
								</div>
							}
						}).collect_view()
					})
				}</div>}.into_any()
			}
		}}.into_any()
	}

	fn refresh_time(&self) -> u64 {
		1000*60*60
	}

	fn refresh(&self, moduleActions: ModuleActionFn, currentName: String) {
		let config = self.config.clone();
		let refreshContent = self.weatherContent.clone();
		spawn_local(async move {
			sync_weather_api(config,refreshContent).await;
		});
	}

	fn export(&self) -> ModuleContent {
		return ModuleContent{
			name: self.typeModule(),
			typeModule: self.typeModule(),
			timestamp: self._update.get_untracked().get(),
			content: serde_json::to_string(&self.config.get_untracked()).unwrap_or_default(),
			pos: [0,0],
			size: [0,0],
		};
	}

	fn import(&mut self, import: ModuleContent)
	{
		let Ok(content): Result<WeatherConfig,_> = serde_json::from_str(&import.content.clone()) else {return};

		self.config.update(|config|{
			*config = content;
		});
		self._update.update(|cache|{
			cache.update_from(import.timestamp);
		});
		self._sended.update(|cache|{
			cache.update_from(import.timestamp);
		});
	}

	fn newFromModuleContent(from: &ModuleContent) -> Option<Self> {
		let Ok(content): Result<WeatherConfig,_> = serde_json::from_str(&from.content) else {return None};
		Some(Self {
			config: ArcRwSignal::new(content),
			weatherContent: Default::default(),
			_update: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
			_sended: ArcRwSignal::new(Cache::newFrom(from.timestamp)),
		})
	}
}

impl Cacheable for Weather
{
	fn cache_mustUpdate(&self) -> bool
	{
		return self._update.get().isNewer(&self._sended.get());
	}

	fn cache_getUpdate(&self) -> ArcRwSignal<Cache> {
		return self._update.clone();
	}

	fn cache_getSended(&self) -> ArcRwSignal<Cache> {
		return self._sended.clone();
	}
}

#[derive(Debug, Clone)]
struct WeatherApiResultOneDay
{
	pub timestampDay: u64,
	pub temp_min: f64,
	pub temp_max: f64,
	pub precipitation: u8,
	pub wind: f64,
	pub code: u8,
}

impl WeatherApiResultOneDay
{
	pub fn codeIntoImg(&self) -> &'static str
	{
		match self.code {
			0 => {"sun"},
			1 => {"cloudy"},
			2 | 3 => {"cloud"},
			51 | 80 | 61 => {"cloudy_rain"},
			53 | 55 | 81 | 63 | 82 | 65 => {"rainy"},
			85 | 71 | 77 => {"ligh_snow"},
			73 | 86 | 75 => {"snowy"},
			45 | 48 => {"foog"},
			95 | 96 | 99 => {"storm"},
			_ => {"sun"}
		}
	}

	pub fn codeIntoTranslate(&self) -> String
	{
		return format!("MODULE_WEATHER_{}",self.codeIntoImg().to_uppercase());
	}
}

#[derive(Debug, Clone)]
struct tempUnit{
	pub temp: String,
	pub precipitation: String,
	pub wind: String,
}

#[derive(Debug, Clone)]
struct WeatherApiResult
{
	pub unit: tempUnit,
	pub days: Vec<WeatherApiResultOneDay>,
	pub lastUpdate: Cache,
}

async fn sync_weather_api(config: ArcRwSignal<WeatherConfig>,weatherContent: ArcRwSignal<Option<WeatherApiResult>>)
{
	if let Some(lastContent) = (weatherContent.clone().get_untracked())
	{
		if(lastContent.lastUpdate.get()	- Cache::now() < 1000*60*5)
		{
			return;
		}
	}

	let config = config.get_untracked();
	let url = format!(
		"https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&daily=temperature_2m_min,temperature_2m_max,precipitation_probability_max,wind_speed_10m_max,weather_code&timezone=auto&forecast_days={}&wind_speed_unit=ms&format=json&timeformat=unixtime",
		config.latitude, config.longitude, config.maxday
	);

	/* result =
	{
		"latitude":43.62,
		"longitude":3.8799996,
		"generationtime_ms":0.12803077697753906,
		"utc_offset_seconds":3600,
		"timezone":"Europe/Paris",
		"timezone_abbreviation":"GMT+1",
		"elevation":58.0,
		"daily_units":{
			"time":"unixtime",
			"temperature_2m_min":"°C",
			"temperature_2m_max":"°C",
			"precipitation_probability_max":"%",
			"wind_speed_10m_max":"m/s"
		},
		"daily":{
			"time":[
				1764975600,
				1765062000,
				1765148400
			],
			"temperature_2m_min":[
				3.6,
				8.5,
				6.8
			],
			"temperature_2m_max":[
				12.2,
				17.5,
				16.3
			],
			"precipitation_probability_max":[
				38,
				5,
				1
			],
			"wind_speed_10m_max":[
				2.12,
				2.01,
				1.89
			]
		}
	}*/

	/*let Ok(window) = web_sys::window().ok_or("no window") else {return};
	let Ok(resp_value) = JsFuture::from(window.fetch_with_str(&url)).await else {return};
	let Ok(resp): Result<Response,_> = resp_value.dyn_into() else {return};

	// On récupère le body en texte (JSON)
	let Ok(text_promise) = resp.text().map_err(|_| "text() failed") else {return};
	let Ok(text_js) = JsFuture::from(text_promise).await else {return};
	let Ok(text) = text_js
		.as_string()
		.ok_or("response text is not a string") else {return};*/

	let mut weatherResult = WeatherApiResult{
		unit: tempUnit {
			temp: "".to_string(),
			precipitation: "".to_string(),
			wind: "".to_string(),
		},
		days: vec![],
		lastUpdate: Default::default(),
	};

	// debug
	let text = "{\"latitude\":43.62,\"longitude\":3.8799996,\"generationtime_ms\":0.13124942779541016,\"utc_offset_seconds\":3600,\"timezone\":\"Europe/Paris\",\"timezone_abbreviation\":\"GMT+1\",\"elevation\":58.0,\"daily_units\":{\"time\":\"unixtime\",\"temperature_2m_min\":\"°C\",\"temperature_2m_max\":\"°C\",\"precipitation_probability_max\":\"%\",\"wind_speed_10m_max\":\"m/s\",\"weather_code\":\"wmo code\"},\"daily\":{\"time\":[1764975600,1765062000,1765148400],\"temperature_2m_min\":[3.6,8.5,7.2],\"temperature_2m_max\":[11.6,16.8,16.5],\"precipitation_probability_max\":[38,5,0],\"wind_speed_10m_max\":[1.71,1.80,2.58],\"weather_code\":[80,45,45]}}".to_string();

	let mut data = text.into_bytes();
	match simd_json::to_borrowed_value(&mut data) {
		Ok(Value::Object(obj)) => {
			HWebTrace!("parsed");
			for (key, value) in obj.into_iter() {
				match key.as_ref() {
					"daily_units" => {
						json_read_daily_units(&mut weatherResult,&value);
					}
					"daily" => {
						json_read_daily(&mut weatherResult,&value);
					}
					_ => {}
				}
			}
		}
		Err(e) => {
			HWebTrace!("error : {:?}", e);
		}
		_ => {}
	}

	weatherContent.update(|content|{
		*content = Some(weatherResult);
	});
}

fn json_read_daily(result: &mut WeatherApiResult, json: &Value)
{
	let mut times = vec![];
	let mut temperature_2m_min = vec![];
	let mut temperature_2m_max = vec![];
	let mut precipitation_probability_max = vec![];
	let mut wind_speed_10m_max = vec![];
	let mut code = vec![];

	let Value::Object(obj) = json else {return};
	for (key, value) in obj.iter() {
		match key.as_ref() {
			"time" => {
				if let Value::Array(value) = value {
					times = value.iter()
						.map(|v| {v.as_u64().unwrap_or(0)})
						.collect::<Vec<u64>>();
				}
			}
			"temperature_2m_min" => {
				if let Value::Array(value) = value {
					temperature_2m_min = value.iter()
						.map(|v| v.as_f64().unwrap_or(0.0))
						.collect::<Vec<f64>>();
				}
			}
			"temperature_2m_max" => {
				if let Value::Array(value) = value {
					temperature_2m_max = value.iter()
						.map(|v| v.as_f64().unwrap_or(0.0))
						.collect::<Vec<f64>>();
				}
			}
			"precipitation_probability_max" => {
				if let Value::Array(value) = value {
					precipitation_probability_max = value.iter()
						.map(|v| v.as_u8().unwrap_or(0))
						.collect::<Vec<u8>>();
				}
			}
			"wind_speed_10m_max" => {
				if let Value::Array(value) = value {
					wind_speed_10m_max = value.iter()
						.map(|v| v.as_f64().unwrap_or(0.0))
						.collect::<Vec<f64>>();
				}
			}
			"weather_code" => {
				if let Value::Array(value) = value {
					code = value.iter()
						.map(|v| v.as_u8().unwrap_or(0))
						.collect::<Vec<u8>>();
				}
			}
			_ => {}
		}
	}

	result.days = times.iter().enumerate().map(|(i,time)|{
		return WeatherApiResultOneDay{
			timestampDay: *time,
			temp_min: temperature_2m_min[i],
			temp_max: temperature_2m_max[i],
			precipitation: precipitation_probability_max[i],
			wind: wind_speed_10m_max[i],
			code: code[i],
		};
	}).collect::<Vec<WeatherApiResultOneDay>>();
}

fn json_read_daily_units(result: &mut WeatherApiResult, json: &Value)
{
	/*
		"time":"unixtime",
		"temperature_2m_min":"°C",
		"temperature_2m_max":"°C",
		"precipitation_probability_max":"%",
		"wind_speed_10m_max":"m/s"
	 */
	let Value::Object(obj) = json else {return};
	for (key, value) in obj.iter() {
		match key.as_ref() {
			"temperature_2m_min" => {
				if let Value::String(value) = value {
					result.unit.temp = value.to_string();
				}
			}
			"precipitation_probability_max" => {
				if let Value::String(value) = value {
					result.unit.precipitation = value.to_string();
				}
			}
			"wind_speed_10m_max" => {

				if let Value::String(value) = value {
					result.unit.wind = value.to_string();
				}
			}
			_ => {}
		}
	}
}