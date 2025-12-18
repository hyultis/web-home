use js_sys::{Array, Intl, Object, Reflect};
use leptoaster::ToasterContext;
use leptos::prelude::{ClassAttribute, CollectView, ElementChild, Get, GetUntracked, OnAttribute, StyleAttribute, Update};
use leptos::prelude::{AnyView, ArcRwSignal, IntoAny, RwSignal};
use leptos::view;
use serde::{Deserialize, Serialize};
use simd_json::borrowed::Value;
use simd_json::prelude::ValueAsScalar;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture};
use web_sys::{window, Position, PositionError, Response};
use crate::api::modules::components::ModuleContent;
use crate::front::modules::components::{Backable, BoxFuture, Cache, Cacheable, FieldHelper, ModuleSizeContrainte, RefreshTime};
use crate::front::modules::module_actions::ModuleActionFn;
use crate::front::utils::translate::Translate;
use crate::HWebTrace;
use time::UtcDateTime;
use wasm_bindgen::prelude::Closure;
use crate::front::utils::draw_title_if_present;

#[derive(Serialize,Deserialize,Debug)]
#[derive(Clone)]
struct WeatherConfig
{
	pub latitude: f64,
	pub longitude: f64,
	pub maxday: u8,
	#[serde(default)]
	pub title: String,
}
impl Default for WeatherConfig
{
	fn default() -> Self
	{
		Self {
			latitude: 0.0,
			longitude: 0.0,
			maxday: 3,
			title: "".to_string(),
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

				let mut titleF = FieldHelper::new(&self.config,&self._update,"MODULE_TITLE_CONF",
		                                  |d| d.get().title,
		                                  |ev,inner| inner.title = ev.target().value());
				titleF.setFullSize(true);
				let latitudeF = FieldHelper::new(&self.config,&self._update,"MODULE_WEATHER_POSITION",
		                                  |d| d.get().latitude.to_string(),
		                                  |ev,inner| inner.latitude = ev.target().value().parse::<f64>().unwrap_or(0.0));
				let longitudeF = FieldHelper::new(&self.config,&self._update,"",
		                                  |d| d.get().longitude.to_string(),
		                                  |ev,inner| inner.longitude = ev.target().value().parse::<f64>().unwrap_or(0.0));
				let maxdayF = FieldHelper::new(&self.config,&self._update,"MODULE_WEATHER_MAXDAY",
		                                  |d| d.get().maxday.to_string(),
		                                  |ev,inner| inner.maxday = ev.target().value().parse::<u8>().unwrap_or(0));
				view!{
				<div class="module_weather_config">
					{titleF.draw()}<br/>
					{latitudeF.draw()}/
					{longitudeF.draw()}<br/>
					<button on:click={locateFn}><Translate key="MODULE_WEATHER_LOCATE"/></button><br/>
					{maxdayF.draw()}
				</div>
				}.into_any()
			}
			else
			{
				let config = self.config.clone();
				view!{
					{draw_title_if_present(config.get().title.clone())}
					<div class="module_weather">{
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
									<i class="iconoir-wind"/>{" "}{days.wind}{units.clone().wind}<br/>
									<i class="iconoir-heavy-rain"/>{" "}{days.precipitation}{units.clone().precipitation}
								</div>
							}
						}).collect_view()
					})
				}</div>}.into_any()
			}
		}}.into_any()
	}

	fn refresh_time(&self) -> RefreshTime {
		RefreshTime::HOURS(1)
	}

	fn refresh(&self, moduleActions: ModuleActionFn, currentName: String, toaster: ToasterContext) -> Option<BoxFuture> {
		let config = self.config.clone();
		let refreshContent = self.weatherContent.clone();

		return Some(Box::pin(async move {
			sync_weather_api(config,refreshContent).await;
		}));
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

	fn size(&self) -> ModuleSizeContrainte {
		let mut minsize = 175;
		if(!self.config.get_untracked().title.is_empty()) {minsize = 210};

		ModuleSizeContrainte{
			x_min: Some(150),
			x_max: None,
			y_min: Some(minsize),
			y_max: None,
		}
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
			1 | 2 | 3 => {"cloudy"},
			45 | 48 => {"fog"},
			51 | 53 | 55 => {"cloudy_rain"},
			56 | 57 => {"light_snow"},
			61 | 63 | 65 => {"cloudy_rain"},
			66 | 67 => {"light_snow"},
			71 | 73 | 75 => {"snow"},
			77 => {"snow_grain"},
			80 | 81 | 82 => {"rain"},
			85 | 86 => {"snow"},
			95 => {"storm"},
			96 | 99 => {"heavystorm"},
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
		if(Cache::now() - lastContent.lastUpdate.get() < 30*1_000_000_000)
		{
			return;
		}
	}

	let options = Intl::DateTimeFormat::new(&Array::new(), &Object::new()).resolved_options();
	let mut timezone = "".to_string();
	if let Ok(reflect) = Reflect::get(&options, &JsValue::from("timeZone"))
	{
		if let Some(timezoneRaw) = reflect.as_string()
		{
			timezone = format!("&timezone={}",timezoneRaw);
		}
	}

	let config = config.get_untracked();
	let url = format!(
		"https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&daily=apparent_temperature_min,apparent_temperature_max,precipitation_probability_max,wind_speed_10m_max,weather_code{}&forecast_days={}&wind_speed_unit=ms&format=json&timeformat=unixtime",
		config.latitude, config.longitude, timezone, config.maxday
	);

	let text = if(false)
	{
		// debug
		"{\"latitude\":42.98,\"longitude\":3.12,\"generationtime_ms\":0.26226043701171875,\"utc_offset_seconds\":3600,\"timezone\":\"Europe/Paris\",\"timezone_abbreviation\":\"GMT+1\",\"elevation\":81.0,\"daily_units\":{\"time\":\"unixtime\",\"apparent_temperature_min\":\"°C\",\"apparent_temperature_max\":\"°C\",\"precipitation_probability_max\":\"%\",\"wind_speed_10m_max\":\"m/s\",\"weather_code\":\"wmo code\"},\"daily\":{\"time\":[1764975600,1765062000,1765148400,1765234800,1765321200,1765407600,1765494000],\"apparent_temperature_min\":[3.7,8.4,7.0,10.7,10.6,6.4,6.1],\"apparent_temperature_max\":[11.4,17.4,16.5,14.3,14.4,17.3,12.3],\"precipitation_probability_max\":[38,5,0,58,51,14,42],\"wind_speed_10m_max\":[2.10,2.27,2.36,3.98,2.84,1.87,3.36],\"weather_code\":[80,45,45,80,80,45,3]}}".to_string()
	}
	else
	{
		let Ok(window) = web_sys::window().ok_or("no window") else {return};
		let Ok(resp_value) = JsFuture::from(window.fetch_with_str(&url)).await else {return};
		let Ok(resp): Result<Response,_> = resp_value.dyn_into() else {return};

		// On récupère le body en texte (JSON)
		let Ok(text_promise) = resp.text().map_err(|_| "text() failed") else {return};
		let Ok(text_js) = JsFuture::from(text_promise).await else {return};
		let Some(text) = text_js.as_string() else {return};
		text
	};

	let mut weatherResult = WeatherApiResult{
		unit: tempUnit {
			temp: "".to_string(),
			precipitation: "".to_string(),
			wind: "".to_string(),
		},
		days: vec![],
		lastUpdate: Default::default(),
	};

	let mut data = text.into_bytes();
	match simd_json::to_borrowed_value(&mut data) {
		Ok(Value::Object(obj)) => {
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
			// TODO
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
	let mut apparent_temperature_min = vec![];
	let mut apparent_temperature_max = vec![];
	let mut precipitation_probability_max = vec![];
	let mut wind_speed_10m_max = vec![];
	let mut code = vec![];

	let Value::Object(obj) = json else {return};
	for (key, value) in obj.iter() {
		match key.as_ref() {
			"time" => {
				if let Value::Array(value) = value {
					times = value.iter()
						.map(|v| {v.as_u64().unwrap_or(0)+(12*3600)})
						.collect::<Vec<u64>>();
				}
			}
			"apparent_temperature_min" => {
				if let Value::Array(value) = value {
					apparent_temperature_min = value.iter()
						.map(|v| v.as_f64().unwrap_or(0.0))
						.collect::<Vec<f64>>();
				}
			}
			"apparent_temperature_max" => {
				if let Value::Array(value) = value {
					apparent_temperature_max = value.iter()
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
			temp_min: apparent_temperature_min[i],
			temp_max: apparent_temperature_max[i],
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
		"apparent_temperature_min":"°C",
		"apparent_temperature_max":"°C",
		"precipitation_probability_max":"%",
		"wind_speed_10m_max":"m/s"
	 */
	let Value::Object(obj) = json else {return};
	for (key, value) in obj.iter() {
		match key.as_ref() {
			"apparent_temperature_min" => {
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